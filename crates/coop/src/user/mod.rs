use std::str::FromStr;

use anyhow::{anyhow, Error};
use common::nip96_upload;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, App, AppContext, Context, Entity, Flatten, IntoElement, ParentElement,
    PathPromptOptions, Render, SharedString, Styled, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::fs;
use state::NostrRegistry;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::{v_flex, ContextModal, Disableable, IconName, Sizable};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<UserProfile> {
    cx.new(|cx| UserProfile::new(window, cx))
}

#[derive(Debug)]
pub struct UserProfile {
    /// User profile
    profile: Option<Profile>,

    /// User's name text input
    name_input: Entity<InputState>,

    /// User's avatar url text input
    avatar_input: Entity<InputState>,

    /// User's bio multi line input
    bio_input: Entity<InputState>,

    /// User's website url text input
    website_input: Entity<InputState>,

    /// Uploading state
    uploading: bool,

    /// Async operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl UserProfile {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Alice"));
        let avatar_input = cx.new(|cx| InputState::new(window, cx).placeholder("alice.me/a.jpg"));
        let website_input = cx.new(|cx| InputState::new(window, cx).placeholder("alice.me"));

        // Use multi-line input for bio
        let bio_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .auto_grow(3, 8)
                .placeholder("A short introduce about you.")
        });

        let get_profile = Self::get_profile(cx);
        let mut tasks = smallvec![];

        tasks.push(
            // Get metadata in the background
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(profile) = get_profile.await {
                    this.update_in(cx, |this, window, cx| {
                        this.set_profile(profile, window, cx);
                    })
                    .ok();
                }
            }),
        );

        Self {
            profile: None,
            name_input,
            avatar_input,
            bio_input,
            website_input,
            uploading: false,
            _tasks: tasks,
        }
    }

    fn get_profile(cx: &App) -> Task<Result<Profile, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let metadata = client
                .database()
                .metadata(public_key)
                .await?
                .unwrap_or_default();

            Ok(Profile::new(public_key, metadata))
        })
    }

    fn set_profile(&mut self, profile: Profile, window: &mut Window, cx: &mut Context<Self>) {
        let metadata = profile.metadata();

        self.avatar_input.update(cx, |this, cx| {
            if let Some(avatar) = metadata.picture.as_ref() {
                this.set_value(avatar, window, cx);
            }
        });

        self.bio_input.update(cx, |this, cx| {
            if let Some(bio) = metadata.about.as_ref() {
                this.set_value(bio, window, cx);
            }
        });

        self.name_input.update(cx, |this, cx| {
            if let Some(display_name) = metadata.display_name.as_ref() {
                this.set_value(display_name, window, cx);
            }
        });

        self.website_input.update(cx, |this, cx| {
            if let Some(website) = metadata.website.as_ref() {
                this.set_value(website, window, cx);
            }
        });

        self.profile = Some(profile);
        cx.notify();
    }

    fn uploading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.uploading = status;
        cx.notify();
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
                        this.uploading(false, cx);
                        this.avatar_input.update(cx, |this, cx| {
                            this.set_value(url.to_string(), window, cx);
                        });
                    }
                    Ok(Err(e)) => {
                        window.push_notification(e.to_string(), cx);
                        this.uploading(false, cx);
                    }
                    Err(e) => {
                        log::warn!("Failed to upload avatar: {e}");
                        this.uploading(false, cx);
                    }
                };
            })
            .expect("Entity has been released");
        })
        .detach();
    }

    pub fn set_metadata(&mut self, cx: &mut Context<Self>) -> Task<Result<Profile, Error>> {
        let avatar = self.avatar_input.read(cx).value().to_string();
        let name = self.name_input.read(cx).value().to_string();
        let bio = self.bio_input.read(cx).value().to_string();
        let website = self.website_input.read(cx).value().to_string();

        // Get the current profile metadata
        let old_metadata = self
            .profile
            .as_ref()
            .map(|profile| profile.metadata())
            .unwrap_or_default();

        // Construct the new metadata
        let mut new_metadata = old_metadata.display_name(name).about(bio);

        if let Ok(url) = Url::from_str(&avatar) {
            new_metadata = new_metadata.picture(url);
        };

        if let Ok(url) = Url::from_str(&website) {
            new_metadata = new_metadata.website(url);
        }

        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let gossip = nostr.read(cx).gossip();

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let gossip = gossip.read().await;
            let write_relays = gossip.inbox_relays(&public_key);

            // Ensure connections to the write relays
            gossip.ensure_connections(&client, &write_relays).await;

            // Sign the new metadata event
            let event = EventBuilder::metadata(&new_metadata).sign(&signer).await?;

            // Send event to user's write relayss
            client.send_event_to(write_relays, &event).await?;

            // Return the updated profile
            let metadata = Metadata::from_json(&event.content).unwrap_or_default();
            let profile = Profile::new(event.pubkey, metadata);

            Ok(profile)
        })
    }
}

impl Render for UserProfile {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .w_full()
                    .h_32()
                    .bg(cx.theme().surface_background)
                    .rounded(cx.theme().radius)
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .map(|this| {
                        let picture = self.avatar_input.read(cx).value();
                        if picture.is_empty() {
                            this.child(
                                img("brand/avatar.png")
                                    .rounded_full()
                                    .size_10()
                                    .flex_shrink_0(),
                            )
                        } else {
                            this.child(
                                img(picture.clone())
                                    .rounded_full()
                                    .size_10()
                                    .flex_shrink_0(),
                            )
                        }
                    })
                    .child(
                        Button::new("upload")
                            .icon(IconName::Upload)
                            .label(t!("common.change"))
                            .ghost()
                            .small()
                            .disabled(self.uploading)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.upload(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child(shared_t!("profile.label_name"))
                    .child(TextInput::new(&self.name_input).small()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child(shared_t!("profile.label_website"))
                    .child(TextInput::new(&self.website_input).small()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child(shared_t!("profile.label_bio"))
                    .child(TextInput::new(&self.bio_input).small()),
            )
    }
}
