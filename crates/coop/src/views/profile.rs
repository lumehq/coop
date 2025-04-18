use async_utility::task::spawn;
use common::nip96_upload;
use global::{constants::IMAGE_SERVICE, get_client};
use gpui::{
    div, img, prelude::FluentBuilder, px, App, AppContext, Context, Entity, Flatten, IntoElement,
    ParentElement, PathPromptOptions, Render, Styled, Task, Window,
};
use nostr_sdk::prelude::*;
use smol::fs;
use std::{str::FromStr, time::Duration};
use ui::{
    button::{Button, ButtonVariants},
    input::TextInput,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Disableable, IconName, Sizable, Size,
};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Profile> {
    Profile::new(window, cx)
}

pub struct Profile {
    profile: Option<Metadata>,
    name_input: Entity<TextInput>,
    avatar_input: Entity<TextInput>,
    bio_input: Entity<TextInput>,
    website_input: Entity<TextInput>,
    is_loading: bool,
    is_submitting: bool,
}

impl Profile {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let name_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::Small)
                .placeholder("Alice")
        });

        let avatar_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::XSmall)
                .small()
                .placeholder("https://example.com/avatar.jpg")
        });

        let website_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::Small)
                .placeholder("https://your-website.com")
        });

        let bio_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::Small)
                .multi_line()
                .placeholder("A short introduce about you.")
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
                let client = get_client();
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let metadata = client
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?;

                Ok(metadata)
            });

            cx.spawn_in(window, async move |this, cx| {
                if let Ok(Some(metadata)) = task.await {
                    cx.update(|window, cx| {
                        this.update(cx, |this: &mut Profile, cx| {
                            this.avatar_input.update(cx, |this, cx| {
                                if let Some(avatar) = metadata.picture.as_ref() {
                                    this.set_text(avatar, window, cx);
                                }
                            });
                            this.bio_input.update(cx, |this, cx| {
                                if let Some(bio) = metadata.about.as_ref() {
                                    this.set_text(bio, window, cx);
                                }
                            });
                            this.name_input.update(cx, |this, cx| {
                                if let Some(display_name) = metadata.display_name.as_ref() {
                                    this.set_text(display_name, window, cx);
                                }
                            });
                            this.website_input.update(cx, |this, cx| {
                                if let Some(website) = metadata.website.as_ref() {
                                    this.set_text(website, window, cx);
                                }
                            });
                            this.profile = Some(metadata);

                            cx.notify();
                        })
                        .ok();
                    })
                    .ok();
                }
            })
            .detach();

            this
        })
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let avatar_input = self.avatar_input.downgrade();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        // Show loading spinner
        self.set_loading(true, cx);

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    let path = paths.pop().unwrap();

                    if let Ok(file_data) = fs::read(path).await {
                        let (tx, rx) = oneshot::channel::<Url>();

                        spawn(async move {
                            let client = get_client();
                            if let Ok(url) = nip96_upload(client, file_data).await {
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
                                        this.set_text(url.to_string(), window, cx);
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

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Show loading spinner
        self.set_submitting(true, cx);

        let avatar = self.avatar_input.read(cx).text().to_string();
        let name = self.name_input.read(cx).text().to_string();
        let bio = self.bio_input.read(cx).text().to_string();
        let website = self.website_input.read(cx).text().to_string();

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

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = get_client();
            _ = client.set_metadata(&new_metadata).await?;

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_submitting(false, cx);
                        window.push_notification("Your profile has been updated successfully", cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }

    fn set_submitting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_submitting = status;
        cx.notify();
    }
}

impl Render for Profile {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .w_full()
                    .h_32()
                    .bg(cx.theme().base.step(cx, ColorScaleStep::TWO))
                    .rounded(px(cx.theme().radius))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .map(|this| {
                        let picture = self.avatar_input.read(cx).text();

                        if picture.is_empty() {
                            this.child(img("brand/avatar.png").size_10().flex_shrink_0())
                        } else {
                            this.child(
                                img(format!(
                                    "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1",
                                    IMAGE_SERVICE,
                                    self.avatar_input.read(cx).text()
                                ))
                                .size_10()
                                .flex_shrink_0(),
                            )
                        }
                    })
                    .child(
                        Button::new("upload")
                            .icon(IconName::Upload)
                            .label("Change")
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
                    .child("Name:")
                    .child(self.name_input.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child("Website:")
                    .child(self.website_input.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child("Bio:")
                    .child(self.bio_input.clone()),
            )
            .child(
                div().mt_2().w_full().child(
                    Button::new("submit")
                        .label("Update")
                        .primary()
                        .disabled(self.is_loading || self.is_submitting)
                        .loading(self.is_submitting)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.submit(window, cx);
                        })),
                ),
            )
    }
}
