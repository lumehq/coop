use ::nostr::NostrRegistry;
use anyhow::{anyhow, Error};
use common::nip96::nip96_upload;
use gpui::{
    div, relative, rems, AnyElement, App, AppContext, AsyncWindowContext, Context, Entity,
    EventEmitter, Flatten, FocusHandle, Focusable, IntoElement, ParentElement, PathPromptOptions,
    Render, SharedString, Styled, Task, WeakEntity, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use key_store::backend::KeyItem;
use key_store::KeyStore;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smol::fs;
use states::{default_nip17_relays, default_nip65_relays, BOOTSTRAP_RELAYS};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::{divider, v_flex, ContextModal, Disableable, IconName, Sizable, StyledExt};

use crate::views::backup_keys::BackupKeys;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<NewAccount> {
    NewAccount::new(window, cx)
}

pub struct NewAccount {
    name_input: Entity<InputState>,
    avatar_input: Entity<InputState>,
    temp_keys: Entity<Keys>,
    uploading: bool,
    submitting: bool,
    // Panel
    name: SharedString,
    focus_handle: FocusHandle,
}

impl NewAccount {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let temp_keys = cx.new(|_| Keys::generate());
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Alice"));
        let avatar_input = cx.new(|cx| InputState::new(window, cx));

        Self {
            name_input,
            avatar_input,
            temp_keys,
            uploading: false,
            submitting: false,
            name: "New Account".into(),
            focus_handle: cx.focus_handle(),
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submitting(true, cx);

        let keys = self.temp_keys.read(cx).clone();
        let view = cx.new(|cx| BackupKeys::new(&keys, window, cx));
        let weak_view = view.downgrade();
        let current_view = cx.entity().downgrade();

        window.open_modal(cx, move |modal, _window, _cx| {
            let weak_view = weak_view.clone();
            let current_view = current_view.clone();

            modal
                .alert()
                .title(shared_t!("new_account.backup_label"))
                .child(view.clone())
                .button_props(
                    ModalButtonProps::default().ok_text(t!("new_account.backup_download")),
                )
                .on_ok(move |_, window, cx| {
                    weak_view
                        .update(cx, |this, cx| {
                            let current_view = current_view.clone();

                            if let Some(task) = this.backup(window, cx) {
                                cx.spawn_in(window, async move |_, cx| {
                                    task.await;

                                    current_view
                                        .update(cx, |this, cx| {
                                            this.set_signer(cx);
                                        })
                                        .ok();
                                })
                                .detach();
                            }
                        })
                        .ok();
                    // true to close the modal
                    false
                })
        })
    }

    pub fn set_signer(&mut self, cx: &mut Context<Self>) {
        let keystore = KeyStore::global(cx).read(cx).backend();

        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let keys = self.temp_keys.read(cx).clone();
        let username = keys.public_key().to_hex();
        let secret = keys.secret_key().to_secret_hex().into_bytes();

        let avatar = self.avatar_input.read(cx).value().to_string();
        let name = self.name_input.read(cx).value().to_string();
        let mut metadata = Metadata::new().display_name(name.clone()).name(name);

        if let Ok(url) = Url::parse(&avatar) {
            metadata = metadata.picture(url);
        };

        cx.spawn(async move |_, cx| {
            let url = KeyItem::User.to_string();

            // Write the app keys for further connection
            keystore
                .write_credentials(&url, &username, &secret, cx)
                .await
                .ok();

            // Update the signer
            // Set the client's signer with the current keys
            let task: Task<Result<(), Error>> = cx.background_spawn(async move {
                // Set the client's signer with the current keys
                client.set_signer(keys).await;

                // Verify the signer
                let signer = client.signer().await?;

                // Construct a NIP-65 event
                let event = EventBuilder::new(Kind::RelayList, "")
                    .tags(
                        default_nip65_relays()
                            .iter()
                            .cloned()
                            .map(|(url, metadata)| Tag::relay_metadata(url, metadata)),
                    )
                    .sign(&signer)
                    .await?;

                // Set NIP-65 relays
                client.send_event_to(BOOTSTRAP_RELAYS, &event).await?;

                // Construct a NIP-17 event
                let event = EventBuilder::new(Kind::InboxRelays, "")
                    .tags(default_nip17_relays().iter().cloned().map(Tag::relay))
                    .sign(&signer)
                    .await?;

                // Set NIP-17 relays
                client.send_event(&event).await?;

                // Construct a metadata event
                let event = EventBuilder::metadata(&metadata).sign(&signer).await?;

                // Set metadata
                client.send_event(&event).await?;

                Ok(())
            });

            task.detach();
        })
        .detach();
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.uploading(true, cx);

        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Get the user's configured NIP96 server
        let nip96_server = AppSettings::get_media_server(cx);

        // Open native file dialog
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        let task = Tokio::spawn(cx, async move {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    if let Some(path) = paths.pop() {
                        let file = fs::read(path).await?;
                        let url = nip96_upload(&client, &nip96_server, file).await?;

                        Ok(url)
                    } else {
                        Err(anyhow!("Path not found"))
                    }
                }
                Ok(None) => Err(anyhow!("User cancelled")),
                Err(e) => Err(anyhow!("File dialog error: {e}")),
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(task.await.map_err(|e| e.into())) {
                Ok(Ok(url)) => {
                    this.update_in(cx, |this, window, cx| {
                        this.uploading(false, cx);
                        this.avatar_input.update(cx, |this, cx| {
                            this.set_value(url.to_string(), window, cx);
                        });
                    })
                    .ok();
                }
                Ok(Err(e)) => {
                    Self::notify_error(cx, this, e.to_string());
                }
                Err(e) => {
                    Self::notify_error(cx, this, e.to_string());
                }
            }
        })
        .detach();
    }

    fn notify_error(cx: &mut AsyncWindowContext, entity: WeakEntity<NewAccount>, e: String) {
        cx.update(|window, cx| {
            entity
                .update(cx, |this, cx| {
                    window.push_notification(e, cx);
                    this.uploading(false, cx);
                })
                .ok();
        })
        .ok();
    }

    fn submitting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.submitting = status;
        cx.notify();
    }

    fn uploading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.uploading = status;
        cx.notify();
    }
}

impl Panel for NewAccount {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }
}

impl EventEmitter<PanelEvent> for NewAccount {}

impl Focusable for NewAccount {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NewAccount {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .relative()
            .items_center()
            .justify_center()
            .gap_10()
            .child(
                div()
                    .text_lg()
                    .text_center()
                    .font_semibold()
                    .line_height(relative(1.3))
                    .child(shared_t!("new_account.title")),
            )
            .child(
                v_flex()
                    .w_96()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .text_sm()
                            .child(shared_t!("new_account.name"))
                            .child(TextInput::new(&self.name_input).small()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child(shared_t!("new_account.avatar")))
                            .child(
                                v_flex()
                                    .p_1()
                                    .h_32()
                                    .w_full()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_dashed()
                                    .border_color(cx.theme().border)
                                    .child(
                                        Avatar::new(self.avatar_input.read(cx).value().to_string())
                                            .size(rems(2.25)),
                                    )
                                    .child(
                                        Button::new("upload")
                                            .icon(IconName::Plus)
                                            .label(t!("common.upload"))
                                            .ghost()
                                            .small()
                                            .rounded()
                                            .disabled(self.submitting || self.uploading)
                                            .loading(self.uploading)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.upload(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(divider(cx))
                    .child(
                        Button::new("submit")
                            .label(t!("common.continue"))
                            .primary()
                            .loading(self.submitting)
                            .disabled(self.submitting || self.uploading)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.create(window, cx);
                            })),
                    ),
            )
    }
}
