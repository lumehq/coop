use anyhow::{anyhow, Error};
use common::{default_nip17_relays, default_nip65_relays, nip96_upload, BOOTSTRAP_RELAYS};
use gpui::{
    rems, AnyElement, App, AppContext, Context, Entity, EventEmitter, Flatten, FocusHandle,
    Focusable, IntoElement, ParentElement, PathPromptOptions, Render, SharedString, Styled, Task,
    Window,
};
use gpui_tokio::Tokio;
use key_store::{KeyItem, KeyStore};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smol::fs;
use state::client;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::{divider, v_flex, ContextModal, Disableable, IconName, Sizable};

mod backup;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<NewAccount> {
    cx.new(|cx| NewAccount::new(window, cx))
}

#[derive(Debug)]
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
            name: "Create a new identity".into(),
            focus_handle: cx.focus_handle(),
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submitting(true, cx);

        let keys = self.temp_keys.read(cx).clone();
        let view = backup::init(&keys, window, cx);
        let weak_view = view.downgrade();
        let current_view = cx.entity().downgrade();

        window.open_modal(cx, move |modal, _window, _cx| {
            let weak_view = weak_view.clone();
            let current_view = current_view.clone();

            modal
                .alert()
                .title(SharedString::from(
                    "Backup to avoid losing access to your account",
                ))
                .child(view.clone())
                .button_props(ModalButtonProps::default().ok_text("Download"))
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
        window.close_all_modals(cx);

        // Set the client's signer with the current keys
        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = client();
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

        // Get the user's configured NIP96 server
        let file_server = {
            let settings = AppSettings::settings(cx);
            settings.file_server.clone()
        };

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
                        let client = client();
                        let url = nip96_upload(client, &file_server, file).await?;

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
        let avatar = self.avatar_input.read(cx).value();

        v_flex()
            .size_full()
            .relative()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w_96()
                    .gap_2()
                    .child(
                        v_flex()
                            .h_40()
                            .w_full()
                            .items_center()
                            .justify_center()
                            .gap_4()
                            .child(Avatar::new(avatar).size(rems(4.25)))
                            .child(
                                Button::new("upload")
                                    .icon(IconName::PlusCircleFill)
                                    .label("Add an avatar")
                                    .xsmall()
                                    .ghost()
                                    .rounded()
                                    .disabled(self.uploading)
                                    //.loading(self.uploading)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.upload(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .text_sm()
                            .child(SharedString::from("What should people call you?"))
                            .child(
                                TextInput::new(&self.name_input)
                                    .disabled(self.submitting)
                                    .small(),
                            ),
                    )
                    .child(divider(cx))
                    .child(
                        Button::new("submit")
                            .label("Continue")
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
