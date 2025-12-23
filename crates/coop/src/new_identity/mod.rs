use anyhow::{anyhow, Error};
use common::{default_nip17_relays, default_nip65_relays, nip96_upload, BOOTSTRAP_RELAYS};
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, Flatten, FocusHandle, Focusable, IntoElement,
    ParentElement, PathPromptOptions, Render, SharedString, Styled, Task, Window,
};
use gpui_component::avatar::Avatar;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::dialog::DialogButtonProps;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, IconName, Sizable, WindowExt};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use key_store::{KeyItem, KeyStore};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smol::fs;
use state::NostrRegistry;

use crate::chatspace;

mod backup;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<NewAccount> {
    cx.new(|cx| NewAccount::new(window, cx))
}

#[derive(Debug)]
pub struct NewAccount {
    focus_handle: FocusHandle,

    /// Input for user's display name
    name_input: Entity<InputState>,

    /// Input for user's avatar url (hidden)
    avatar_input: Entity<InputState>,

    /// Newly created account's keys
    temp_keys: Entity<Keys>,

    /// Whether the upload process is in progress
    uploading: bool,

    /// Whether the account creation process is in progress
    submitting: bool,
}

impl NewAccount {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let temp_keys = cx.new(|_| Keys::generate());
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Alice"));
        let avatar_input = cx.new(|cx| InputState::new(window, cx));

        Self {
            name_input,
            avatar_input,
            temp_keys,
            uploading: false,
            submitting: false,
            focus_handle: cx.focus_handle(),
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submitting(true, cx);

        let keys = self.temp_keys.read(cx).clone();
        let view = backup::init(&keys, window, cx);
        let weak_view = view.downgrade();
        let current_view = cx.entity().downgrade();

        window.open_dialog(cx, move |modal, _window, _cx| {
            let weak_view = weak_view.clone();
            let current_view = current_view.clone();

            modal
                .alert()
                .title(shared_t!("new_account.backup_label"))
                .child(view.clone())
                .button_props(
                    DialogButtonProps::default().ok_text(t!("new_account.backup_download")),
                )
                .on_ok(move |_, window, cx| {
                    weak_view
                        .update(cx, |this, cx| {
                            let view = current_view.clone();
                            let task = this.backup(window, cx);

                            cx.spawn_in(window, async move |_this, cx| {
                                let result = task.await;

                                match result {
                                    Ok(_) => {
                                        view.update_in(cx, |this, window, cx| {
                                            this.set_signer(window, cx);
                                        })
                                        .expect("Entity has been released");
                                    }
                                    Err(e) => {
                                        log::error!("Failed to backup: {e}");
                                    }
                                }
                            })
                            .detach();
                        })
                        .ok();
                    // true to close the modal
                    false
                })
        })
    }

    pub fn set_signer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

        // Close all modals if available
        window.close_all_dialogs(cx);

        // Set the client's signer with the current keys
        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let signer = keys.clone();
            let nip65_relays = default_nip65_relays();
            let nip17_relays = default_nip17_relays();

            // Construct a NIP-65 event
            let event = EventBuilder::new(Kind::RelayList, "")
                .tags(
                    nip65_relays
                        .iter()
                        .cloned()
                        .map(|(url, metadata)| Tag::relay_metadata(url, metadata)),
                )
                .sign(&signer)
                .await?;

            // Set NIP-65 relays
            client.send_event_to(BOOTSTRAP_RELAYS, &event).await?;

            // Extract only write relays
            let write_relays: Vec<RelayUrl> = nip65_relays
                .iter()
                .filter_map(|(url, metadata)| {
                    if metadata.is_none() || metadata == &Some(RelayMetadata::Write) {
                        Some(url.to_owned())
                    } else {
                        None
                    }
                })
                .collect();

            // Ensure relays are connected
            for url in write_relays.iter() {
                client.add_relay(url).await?;
                client.connect_relay(url).await?;
            }

            // Construct a NIP-17 event
            let event = EventBuilder::new(Kind::InboxRelays, "")
                .tags(nip17_relays.iter().cloned().map(Tag::relay))
                .sign(&signer)
                .await?;

            // Set NIP-17 relays
            client.send_event_to(&write_relays, &event).await?;

            // Construct a metadata event
            let event = EventBuilder::metadata(&metadata).sign(&signer).await?;

            // Send metadata event to both write relays and bootstrap relays
            client.send_event_to(&write_relays, &event).await?;
            client.send_event_to(BOOTSTRAP_RELAYS, &event).await?;

            // Update the client's signer with the current keys
            client.set_signer(keys).await;

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            let url = KeyItem::User.to_string();

            // Write the app keys for further connection
            keystore
                .write_credentials(&url, &username, &secret, cx)
                .await
                .ok();

            if let Err(e) = task.await {
                this.update_in(cx, |this, window, cx| {
                    this.submitting(false, cx);
                    window.push_notification(e.to_string(), cx);
                })
                .expect("Entity has been released");
            }
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
            let result = Flatten::flatten(task.await.map_err(|e| e.into()));

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Ok(url)) => {
                        this.avatar_input.update(cx, |this, cx| {
                            this.set_value(url.to_string(), window, cx);
                        });
                    }
                    Ok(Err(e)) => {
                        window.push_notification(e.to_string(), cx);
                    }
                    Err(e) => {
                        log::warn!("Failed to upload avatar: {e}");
                    }
                };
                this.uploading(false, cx);
            })
            .expect("Entity has been released");
        })
        .detach();
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
    fn panel_name(&self) -> &'static str {
        "NewAccount"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .text_xs()
            .gap_2()
            .child(
                Button::new("back")
                    .icon(IconName::ArrowLeft)
                    .small()
                    .ghost()
                    .on_click(|_ev, window, cx| {
                        chatspace::onboarding(window, cx);
                    }),
            )
            .child("Create a new identity")
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
        let avatar = self.avatar_input.read(cx).value();

        v_flex()
            .size_full()
            .relative()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w_96()
                    .gap_3()
                    .child(
                        v_flex()
                            .h_40()
                            .w_full()
                            .items_center()
                            .justify_center()
                            .gap_4()
                            .child(Avatar::new().src(avatar).large())
                            .child(
                                Button::new("upload")
                                    .icon(IconName::Plus)
                                    .label("Add an avatar")
                                    .xsmall()
                                    .ghost()
                                    .rounded(cx.theme().radius)
                                    .disabled(self.uploading)
                                    .loading(self.uploading)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.upload(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from("What should people call you?"))
                            .child(Input::new(&self.name_input).disabled(self.submitting)),
                    )
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
