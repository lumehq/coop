use std::str::FromStr;
use std::time::Duration;

use anyhow::Error;
use common::nip96::nip96_upload;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, App, AppContext, Context, Entity, Flatten, IntoElement, ParentElement,
    PathPromptOptions, Render, SharedString, Styled, Task, Window,
};
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smol::fs;
use states::app_state;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::{v_flex, Disableable, IconName, Sizable};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<EditProfile> {
    EditProfile::new(window, cx)
}

pub struct EditProfile {
    profile: Option<Metadata>,
    name_input: Entity<InputState>,
    avatar_input: Entity<InputState>,
    bio_input: Entity<InputState>,
    website_input: Entity<InputState>,
    is_loading: bool,
    is_submitting: bool,
}

impl EditProfile {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("profile.placeholder_name")));
        let avatar_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/avatar.jpg"));
        let website_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://your-website.com"));
        let bio_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .placeholder(t!("profile.placeholder_bio"))
        });

        cx.new(|cx| {
            let this = Self {
                name_input,
                avatar_input,
                bio_input,
                website_input,
                profile: None,
                is_loading: false,
                is_submitting: false,
            };

            let task: Task<Result<Option<Metadata>, Error>> = cx.background_spawn(async move {
                let client = app_state().client();
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let metadata = client
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?;

                Ok(metadata)
            });

            cx.spawn_in(window, async move |this, cx| {
                if let Ok(Some(metadata)) = task.await {
                    this.update_in(cx, |this: &mut EditProfile, window, cx| {
                        this.avatar_input.update(cx, |this, cx| {
                            if let Some(avatar) = metadata.picture.as_ref() {
                                this.set_value(avatar, window, cx);
                            }
                        });
                        this.bio_input.update(cx, |this, cx| {
                            if let Some(bio) = metadata.about.as_ref() {
                                this.set_value(bio, window, cx);
                            }
                        });
                        this.name_input.update(cx, |this, cx| {
                            if let Some(display_name) = metadata.display_name.as_ref() {
                                this.set_value(display_name, window, cx);
                            }
                        });
                        this.website_input.update(cx, |this, cx| {
                            if let Some(website) = metadata.website.as_ref() {
                                this.set_value(website, window, cx);
                            }
                        });
                        this.profile = Some(metadata);
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();

            this
        })
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let nip96 = AppSettings::get_media_server(cx);
        let avatar_input = self.avatar_input.downgrade();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        // Show loading spinner
        self.set_loading(true, cx);

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    let path = paths.pop().unwrap();

                    if let Ok(file_data) = fs::read(path).await {
                        let (tx, rx) = oneshot::channel::<Url>();

                        nostr_sdk::async_utility::task::spawn(async move {
                            if let Ok(url) =
                                nip96_upload(app_state().client(), &nip96, file_data).await
                            {
                                _ = tx.send(url);
                            }
                        });

                        if let Ok(url) = rx.await {
                            cx.update(|window, cx| {
                                // Stop loading spinner
                                this.update(cx, |this, cx| {
                                    this.set_loading(false, cx);
                                })
                                .ok();

                                // Set avatar input
                                avatar_input
                                    .update(cx, |this, cx| {
                                        this.set_value(url.to_string(), window, cx);
                                    })
                                    .ok();
                            })
                            .ok();
                        }
                    }
                }
                Ok(None) => {
                    cx.update(|_, cx| {
                        // Stop loading spinner
                        this.update(cx, |this, cx| {
                            this.set_loading(false, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(_) => {}
            }
        })
        .detach();
    }

    pub fn set_metadata(&mut self, cx: &mut Context<Self>) -> Task<Result<Profile, Error>> {
        let avatar = self.avatar_input.read(cx).value().to_string();
        let name = self.name_input.read(cx).value().to_string();
        let bio = self.bio_input.read(cx).value().to_string();
        let website = self.website_input.read(cx).value().to_string();

        let old_metadata = if let Some(metadata) = self.profile.as_ref() {
            metadata.clone()
        } else {
            Metadata::default()
        };

        let mut new_metadata = old_metadata.display_name(name).about(bio);

        if let Ok(url) = Url::from_str(&avatar) {
            new_metadata = new_metadata.picture(url);
        };

        if let Ok(url) = Url::from_str(&website) {
            new_metadata = new_metadata.website(url);
        }

        cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;

            // Sign the new metadata event
            let event = EventBuilder::metadata(&new_metadata).sign(&signer).await?;

            // Send event to user's write relayss
            client.send_event(&event).await?;

            // Return the updated profile
            let metadata = Metadata::from_json(&event.content).unwrap_or_default();
            let profile = Profile::new(event.pubkey, metadata);

            Ok(profile)
        })
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }
}

impl Render for EditProfile {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                div()
                    .w_full()
                    .h_32()
                    .bg(cx.theme().surface_background)
                    .rounded(cx.theme().radius)
                    .flex()
                    .flex_col()
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
                            .disabled(self.is_loading || self.is_submitting)
                            .loading(self.is_loading)
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
