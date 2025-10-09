use anyhow::anyhow;
use common::nip96::nip96_upload;
use global::constants::{ACCOUNT_PATH, NIP17_RELAYS, NIP65_RELAYS};
use global::nostr_client;
use gpui::{
    div, relative, rems, AnyElement, App, AppContext, AsyncWindowContext, Context, Entity,
    EventEmitter, Flatten, FocusHandle, Focusable, IntoElement, ParentElement, PathPromptOptions,
    Render, SharedString, Styled, WeakEntity, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smol::fs;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::popup_menu::PopupMenu;
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
                            let password = this.password(cx);
                            let current_view = current_view.clone();

                            if let Some(task) = this.backup(window, cx) {
                                cx.spawn_in(window, async move |_, cx| {
                                    task.await;

                                    cx.update(|window, cx| {
                                        current_view
                                            .update(cx, |this, cx| {
                                                this.set_signer(password, window, cx);
                                            })
                                            .ok();
                                    })
                                    .ok()
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

    fn set_signer(&mut self, password: String, window: &mut Window, cx: &mut Context<Self>) {
        window.close_modal(cx);

        let keys = self.temp_keys.read(cx).clone();
        let avatar = self.avatar_input.read(cx).value().to_string();
        let name = self.name_input.read(cx).value().to_string();
        let mut metadata = Metadata::new().display_name(name.clone()).name(name);

        if let Ok(url) = Url::parse(&avatar) {
            metadata = metadata.picture(url);
        };

        // Encrypt and save user secret key to disk
        self.write_keys_to_disk(&keys, password, cx);

        // Set the client's signer with the current keys
        cx.background_spawn(async move {
            let client = nostr_client();

            // Set the client's signer with the current keys
            client.set_signer(keys).await;

            // Set metadata
            if let Err(e) = client.set_metadata(&metadata).await {
                log::error!("Failed to set metadata: {e}");
            }

            // Set NIP-65 relays
            let builder = EventBuilder::new(Kind::RelayList, "").tags(
                NIP65_RELAYS.into_iter().filter_map(|url| {
                    if let Ok(url) = RelayUrl::parse(url) {
                        Some(Tag::relay_metadata(url, None))
                    } else {
                        None
                    }
                }),
            );

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send NIP-65 relay list event: {e}");
            }

            // Set NIP-17 relays
            let builder = EventBuilder::new(Kind::InboxRelays, "").tags(
                NIP17_RELAYS.into_iter().filter_map(|url| {
                    if let Ok(url) = RelayUrl::parse(url) {
                        Some(Tag::relay(url))
                    } else {
                        None
                    }
                }),
            );

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send messaging relay list event: {e}");
            };
        })
        .detach();
    }

    fn write_keys_to_disk(&self, keys: &Keys, password: String, cx: &mut Context<Self>) {
        let keys = keys.to_owned();
        let public_key = keys.public_key();

        cx.background_spawn(async move {
            if let Ok(enc_key) =
                EncryptedSecretKey::new(keys.secret_key(), &password, 8, KeySecurity::Unknown)
            {
                let client = nostr_client();
                let value = enc_key.to_bech32().unwrap();

                let builder = EventBuilder::new(Kind::ApplicationSpecificData, value)
                    .tag(Tag::identifier(ACCOUNT_PATH))
                    .build(public_key)
                    .sign(&Keys::generate())
                    .await;

                if let Ok(event) = builder {
                    if let Err(e) = client.database().save_event(&event).await {
                        log::error!("Failed to save event: {e}");
                    };
                }
            }
        })
        .detach();
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.uploading(true, cx);

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
                        let url = nip96_upload(nostr_client(), &nip96_server, file).await?;

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

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
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
