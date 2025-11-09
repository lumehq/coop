use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use encryption::Encryption;
use futures::FutureExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window,
};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::{h_flex, v_flex, Disableable, Sizable, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<EncryptionPanel> {
    cx.new(|cx| EncryptionPanel::new(window, cx))
}

#[derive(Debug)]
pub struct EncryptionPanel {
    /// Whether the panel is currently requesting encryption.
    requesting: bool,

    /// Whether the panel is currently resetting encryption.
    resetting: bool,

    /// Whether the panel is currently showing an error.
    error: Entity<Option<SharedString>>,
}

impl EncryptionPanel {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let error = cx.new(|_| None);

        Self {
            requesting: false,
            resetting: false,
            error,
        }
    }

    fn set_requesting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.requesting = status;
        cx.notify();
    }

    fn set_resetting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.resetting = status;
        cx.notify();
    }

    fn set_error(&mut self, error: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.error.update(cx, |this, cx| {
            *this = Some(error.into());
            cx.notify();
        });
    }

    fn request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let encryption = Encryption::global(cx);
        let send_request = encryption.read(cx).send_request(cx);

        // Ensure the user has not sent multiple requests
        if self.requesting {
            return;
        }
        self.set_requesting(true, cx);

        cx.spawn_in(window, async move |this, cx| {
            match send_request.await {
                Ok(Some(keys)) => {
                    this.update(cx, |this, cx| {
                        this.set_requesting(false, cx);
                        // Set the encryption key
                        encryption.update(cx, |this, cx| {
                            this.set_encryption(Arc::new(keys), cx);
                        });
                    })
                    .expect("Entity has been released");
                }
                Ok(None) => {
                    this.update_in(cx, |this, window, cx| {
                        this.wait_for_approval(window, cx);
                    })
                    .expect("Entity has been released");
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.set_requesting(false, cx);
                        this.set_error(e.to_string(), cx);
                    })
                    .expect("Entity has been released");
                }
            }
        })
        .detach();
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let encryption = Encryption::global(cx);
        let reset = encryption.read(cx).new_encryption(cx);

        cx.spawn_in(window, async move |this, cx| {
            match reset.await {
                Ok(keys) => {
                    this.update(cx, |this, cx| {
                        this.set_resetting(false, cx);
                        // Set the encryption key
                        encryption.update(cx, |this, cx| {
                            this.set_encryption(Arc::new(keys), cx);
                        });
                    })
                    .expect("Entity has been released");
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.set_resetting(false, cx);
                        this.set_error(e.to_string(), cx);
                    })
                    .expect("Entity has been released");
                }
            }
        })
        .detach();
    }

    fn wait_for_approval(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let encryption = Encryption::global(cx);
        let wait_for_approval = encryption.read(cx).wait_for_approval(cx);

        cx.spawn_in(window, async move |this, cx| {
            let timeout = cx.background_executor().timer(Duration::from_secs(30));

            let result = futures::select! {
                result = wait_for_approval.fuse() => {
                    // Ok(keys)
                    result
                },
                _ = timeout.fuse() => {
                    Err(anyhow!("Timeout"))
                }
            };

            this.update(cx, |this, cx| {
                match result {
                    Ok(keys) => {
                        this.set_requesting(false, cx);
                        // Set the encryption key
                        encryption.update(cx, |this, cx| {
                            this.set_encryption(Arc::new(keys), cx);
                        });
                    }
                    Err(e) => {
                        this.set_error(e.to_string(), cx);
                    }
                };
            })
            .expect("Entity has been released");
        })
        .detach();
    }
}

impl Render for EncryptionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const NOTICE: &str = "You've set up Encryption Key on other client.";
        const DESCRIPTION: &str = "Encryption Key is used to replace the User's Identity in encryption and decryption processes.";
        const WARNING: &str = "Encryption Key is still in the alpha stage. Please be cautious.";

        let encryption = Encryption::global(cx);

        v_flex()
            .p_2()
            .gap_1p5()
            .max_w(px(360.))
            .text_sm()
            .map(|this| {
                if let Some(announcement) = encryption.read(cx).announcement().as_ref() {
                    this.child(SharedString::from(NOTICE))
                        .child(
                            v_flex()
                                .h_12()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .bg(cx.theme().elevated_surface_background)
                                .child(announcement.client()),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .when(!self.requesting, |this| {
                                    this.child(
                                        Button::new("reset")
                                            .label("Reset")
                                            .flex_1()
                                            .small()
                                            .ghost_alt()
                                            .loading(self.resetting)
                                            .disabled(self.resetting)
                                            .on_click(cx.listener(move |this, _ev, window, cx| {
                                                this.reset(window, cx);
                                            })),
                                    )
                                })
                                .when(!self.resetting, |this| {
                                    this.child(
                                        Button::new("request")
                                            .label({
                                                if self.requesting {
                                                    "Wait for approval"
                                                } else {
                                                    "Request"
                                                }
                                            })
                                            .flex_1()
                                            .small()
                                            .primary()
                                            .loading(self.requesting)
                                            .disabled(self.requesting)
                                            .on_click(cx.listener(move |this, _ev, window, cx| {
                                                this.request(window, cx);
                                            })),
                                    )
                                }),
                        )
                        .when_some(self.error.read(cx).as_ref(), |this, error| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_center()
                                    .text_color(cx.theme().danger_foreground)
                                    .child(error.clone()),
                            )
                        })
                } else {
                    this.child(
                        div()
                            .font_semibold()
                            .text_color(cx.theme().text)
                            .child("Set up Encryption Key"),
                    )
                    .child(SharedString::from(DESCRIPTION))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().warning_foreground)
                            .child(SharedString::from(WARNING)),
                    )
                }
            })
    }
}
