use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use common::shorten_pubkey;
use encryption::Encryption;
use futures::FutureExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use smallvec::{smallvec, SmallVec};
use state::Announcement;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::notification::Notification;
use ui::{h_flex, v_flex, ContextModal, Disableable, Icon, IconName, Sizable, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<EncryptionPanel> {
    cx.new(|cx| EncryptionPanel::new(window, cx))
}

#[derive(Debug)]
pub struct EncryptionPanel {
    /// Whether the panel is currently requesting encryption.
    requesting: bool,

    /// Whether the panel is currently creating encryption.
    creating: bool,

    /// Whether the panel is currently showing an error.
    error: Entity<Option<SharedString>>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,
}

impl EncryptionPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let error = cx.new(|_| None);

        let encryption = Encryption::global(cx);
        let requests = encryption.read(cx).requests();

        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Observe encryption request
            cx.observe_in(&requests, window, |this, state, window, cx| {
                for req in state.read(cx).clone().into_iter() {
                    this.ask_for_approval(req, window, cx);
                }
            }),
        );

        Self {
            requesting: false,
            creating: false,
            error,
            _subscriptions: subscriptions,
        }
    }

    fn set_requesting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.requesting = status;
        cx.notify();
    }

    fn set_creating(&mut self, status: bool, cx: &mut Context<Self>) {
        self.creating = status;
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

    fn new_encryption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let encryption = Encryption::global(cx);
        let reset = encryption.read(cx).new_encryption(cx);

        // Ensure the user has not sent multiple requests
        if self.requesting {
            return;
        }
        self.set_creating(true, cx);

        cx.spawn_in(window, async move |this, cx| {
            match reset.await {
                Ok(keys) => {
                    this.update(cx, |this, cx| {
                        this.set_creating(false, cx);
                        // Set the encryption key
                        encryption.update(cx, |this, cx| {
                            this.set_encryption(Arc::new(keys), cx);
                            this.listen_request(cx);
                        });
                    })
                    .expect("Entity has been released");
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.set_creating(false, cx);
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

    fn ask_for_approval(&mut self, req: Announcement, window: &mut Window, cx: &mut Context<Self>) {
        let client_name = SharedString::from(req.client().to_string());
        let target = req.public_key();

        let note = Notification::new()
            .custom_id(SharedString::from(req.id().to_hex()))
            .autohide(false)
            .icon(IconName::Info)
            .title(SharedString::from("Encryption Key Request"))
            .content(move |_window, cx| {
                v_flex()
                    .gap_2()
                    .text_sm()
                    .child(SharedString::from(
                        "You've requested for the Encryption Key from:",
                    ))
                    .child(
                        v_flex()
                            .py_1()
                            .px_1p5()
                            .rounded_sm()
                            .text_xs()
                            .bg(cx.theme().warning_background)
                            .text_color(cx.theme().warning_foreground)
                            .child(client_name.clone()),
                    )
                    .into_any_element()
            })
            .action(move |_window, _cx| {
                Button::new("approve")
                    .label("Approve")
                    .small()
                    .primary()
                    .loading(false)
                    .disabled(false)
                    .on_click(move |_ev, _window, cx| {
                        let encryption = Encryption::global(cx);
                        let send_response = encryption.read(cx).send_response(target, cx);

                        send_response.detach();
                    })
            });

        // Push the notification to the current window
        window.push_notification(note, cx);

        // Focus the window if it's not active
        if !window.is_window_hovered() {
            window.activate_window();
        }
    }
}

impl Render for EncryptionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const NOTICE: &str = "Found an Encryption Announcement";
        const SUGGEST: &str = "Please request the Encryption Key to continue using.";

        const DESCRIPTION: &str = "Encryption Key is used to replace the User's Identity in encryption and decryption messages. Coop will automatically fallback to User's Identity if needed.";
        const WARNING: &str = "Encryption Key is still in the alpha stage. Please be cautious.";

        let encryption = Encryption::global(cx);
        let has_encryption = encryption.read(cx).has_encryption(cx);

        v_flex()
            .p_2()
            .max_w(px(320.))
            .w(px(320.))
            .text_sm()
            .when(has_encryption, |this| {
                this.child(
                    h_flex()
                        .gap_2()
                        .w_full()
                        .text_xs()
                        .font_semibold()
                        .child(
                            Icon::new(IconName::CheckCircleFill)
                                .small()
                                .text_color(cx.theme().element_active),
                        )
                        .child(SharedString::from("Encryption Key has been set")),
                )
            })
            .when(!has_encryption, |this| {
                if let Some(announcement) = encryption.read(cx).announcement().as_ref() {
                    let pubkey = shorten_pubkey(announcement.public_key(), 16);
                    let name = announcement.client();

                    this.child(
                        v_flex()
                            .gap_2()
                            .child(div().font_semibold().child(SharedString::from(NOTICE)))
                            .child(
                                v_flex()
                                    .h_12()
                                    .items_center()
                                    .justify_center()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().warning_background)
                                    .text_color(cx.theme().warning_foreground)
                                    .child(name),
                            )
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().text_muted)
                                            .child(SharedString::from("Client Public Key:")),
                                    )
                                    .child(
                                        h_flex()
                                            .h_7()
                                            .w_full()
                                            .px_2()
                                            .rounded(cx.theme().radius)
                                            .bg(cx.theme().elevated_surface_background)
                                            .child(SharedString::from(pubkey)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().text_muted)
                                    .child(SharedString::from(SUGGEST)),
                            )
                            .child(
                                h_flex()
                                    .mt_2()
                                    .gap_1()
                                    .when(!self.requesting, |this| {
                                        this.child(
                                            Button::new("reset")
                                                .label("Reset")
                                                .flex_1()
                                                .small()
                                                .ghost_alt()
                                                .loading(self.creating)
                                                .disabled(self.creating)
                                                .on_click(cx.listener(
                                                    move |this, _ev, window, cx| {
                                                        this.new_encryption(window, cx);
                                                    },
                                                )),
                                        )
                                    })
                                    .when(!self.creating, |this| {
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
                                                .on_click(cx.listener(
                                                    move |this, _ev, window, cx| {
                                                        this.request(window, cx);
                                                    },
                                                )),
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
                            }),
                    )
                } else {
                    this.child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .font_semibold()
                                    .child(SharedString::from("Set up Encryption Key")),
                            )
                            .child(SharedString::from(DESCRIPTION))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().warning_foreground)
                                    .child(SharedString::from(WARNING)),
                            )
                            .child(
                                Button::new("create")
                                    .label("Setup")
                                    .flex_1()
                                    .small()
                                    .primary()
                                    .loading(self.creating)
                                    .disabled(self.creating)
                                    .on_click(cx.listener(move |this, _ev, window, cx| {
                                        this.new_encryption(window, cx);
                                    })),
                            ),
                    )
                }
            })
    }
}
