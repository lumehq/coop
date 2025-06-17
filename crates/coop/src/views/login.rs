use std::sync::Arc;
use std::time::Duration;

use client_keys::ClientKeys;
use common::handle_auth::CoopAuthUrlHandler;
use common::string_to_qr;
use global::constants::{APP_NAME, NOSTR_CONNECT_RELAY, NOSTR_CONNECT_TIMEOUT};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, red, relative, AnyElement, App, AppContext, ClipboardItem, Context, Entity,
    EventEmitter, FocusHandle, Focusable, Image, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
};
use identity::Identity;
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::{ContextModal, Disableable, Sizable, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Login> {
    Login::new(window, cx)
}

pub struct Login {
    key_input: Entity<InputState>,
    relay_input: Entity<InputState>,
    connection_string: Entity<NostrConnectURI>,
    qr_image: Entity<Option<Arc<Image>>>,
    // Signer that created by Connection String
    active_signer: Entity<Option<NostrConnect>>,
    // Error for the key input
    error: Entity<Option<SharedString>>,
    is_logging_in: bool,
    // Panel
    name: SharedString,
    focus_handle: FocusHandle,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 5]>,
}

impl Login {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // nsec or bunker_uri (NIP46: https://github.com/nostr-protocol/nips/blob/master/46.md)
        let key_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("nsec... or bunker://..."));

        let relay_input =
            cx.new(|cx| InputState::new(window, cx).default_value(NOSTR_CONNECT_RELAY));

        // NIP46: https://github.com/nostr-protocol/nips/blob/master/46.md
        //
        // Direct connection initiated by the client
        let connection_string = cx.new(|cx| {
            let relay = RelayUrl::parse(NOSTR_CONNECT_RELAY).unwrap();
            let client_keys = ClientKeys::get_global(cx).keys();

            NostrConnectURI::client(client_keys.public_key(), vec![relay], APP_NAME)
        });

        // Convert the Connection String into QR Image
        let qr_image = cx.new(|_| None);
        let async_qr_image = qr_image.downgrade();

        // Keep track of the signer that created by Connection String
        let active_signer = cx.new(|_| None);
        let async_active_signer = active_signer.downgrade();

        let error = cx.new(|_| None);
        let mut subscriptions = smallvec![];

        // Subscribe to key input events and process login when the user presses enter
        subscriptions.push(
            cx.subscribe_in(&key_input, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.login(window, cx);
                }
            }),
        );

        // Subscribe to relay input events and change relay when the user presses enter
        subscriptions.push(
            cx.subscribe_in(&relay_input, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.change_relay(window, cx);
                }
            }),
        );

        // Observe the Connect URI that changes when the relay is changed
        subscriptions.push(cx.observe_new::<NostrConnectURI>(move |uri, _window, cx| {
            let client_keys = ClientKeys::get_global(cx).keys();
            let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT);

            if let Ok(mut signer) = NostrConnect::new(uri.to_owned(), client_keys, timeout, None) {
                // Automatically open auth url
                signer.auth_url_handler(CoopAuthUrlHandler);

                async_active_signer
                    .update(cx, |this, cx| {
                        *this = Some(signer);
                        cx.notify();
                    })
                    .ok();
            }

            // Update the QR Image with the new connection string
            async_qr_image
                .update(cx, |this, cx| {
                    *this = string_to_qr(&uri.to_string());
                    cx.notify();
                })
                .ok();
        }));

        subscriptions.push(cx.observe_in(
            &connection_string,
            window,
            |this, entity, _window, cx| {
                let connection_string = entity.read(cx).clone();
                let client_keys = ClientKeys::get_global(cx).keys();

                // Update the QR Image with the new connection string
                this.qr_image.update(cx, |this, cx| {
                    *this = string_to_qr(&connection_string.to_string());
                    cx.notify();
                });

                match NostrConnect::new(
                    connection_string,
                    client_keys,
                    Duration::from_secs(NOSTR_CONNECT_TIMEOUT),
                    None,
                ) {
                    Ok(mut signer) => {
                        // Automatically open auth url
                        signer.auth_url_handler(CoopAuthUrlHandler);

                        this.active_signer.update(cx, |this, cx| {
                            *this = Some(signer);
                            cx.notify();
                        });
                    }
                    Err(_) => {
                        log::error!("Failed to create Nostr Connect")
                    }
                }
            },
        ));

        subscriptions.push(
            cx.observe_in(&active_signer, window, |this, entity, window, cx| {
                if let Some(mut signer) = entity.read(cx).clone() {
                    // Automatically open auth url
                    signer.auth_url_handler(CoopAuthUrlHandler);
                    // Wait for connection from remote signer
                    this.wait_for_connection(signer, window, cx);
                }
            }),
        );

        Self {
            name: "Login".into(),
            focus_handle: cx.focus_handle(),
            is_logging_in: false,
            key_input,
            relay_input,
            connection_string,
            qr_image,
            error,
            active_signer,
            subscriptions,
        }
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_logging_in {
            return;
        };
        // Prevent duplicate login requests
        self.set_logging_in(true, cx);

        // Content can be secret key or bunker://
        match self.key_input.read(cx).value().to_string() {
            s if s.starts_with("nsec1") => self.ask_for_password(s, window, cx),
            s if s.starts_with("ncryptsec1") => self.ask_for_password(s, window, cx),
            s if s.starts_with("bunker://") => self.login_with_bunker(&s, window, cx),
            _ => self.set_error("You must provide a valid Private Key or Bunker.", cx),
        };
    }

    fn ask_for_password(&mut self, content: String, window: &mut Window, cx: &mut Context<Self>) {
        let current_view = cx.entity().downgrade();
        let pwd_input = cx.new(|cx| InputState::new(window, cx).masked(true));
        let weak_input = pwd_input.downgrade();

        window.open_modal(cx, move |this, _window, cx| {
            let weak_input = weak_input.clone();
            let view_cancel = current_view.clone();
            let view_ok = current_view.clone();

            let label: SharedString = if content.starts_with("nsec1") {
                "Set password to encrypt your key *".into()
            } else {
                "Password to decrypt your key *".into()
            };

            let description: Option<SharedString> = if content.starts_with("ncryptsec1") {
                Some("Coop will only stored the encrypted version".into())
            } else {
                None
            };

            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .on_cancel(move |_, _window, cx| {
                    view_cancel
                        .update(cx, |this, cx| {
                            this.set_error("Password is required", cx);
                        })
                        .ok();
                    true
                })
                .on_ok(move |_, window, cx| {
                    let value = weak_input
                        .read_with(cx, |state, _cx| state.value().to_owned())
                        .ok();

                    view_ok
                        .update(cx, |this, cx| {
                            if let Some(password) = value {
                                this.login_with_keys(password.to_string(), window, cx);
                            } else {
                                this.set_error("Password is required", cx);
                            }
                        })
                        .ok();
                    true
                })
                .child(
                    div()
                        .pt_4()
                        .px_4()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .child(label)
                        .child(TextInput::new(&pwd_input).small())
                        .when_some(description, |this, description| {
                            this.child(
                                div()
                                    .text_xs()
                                    .italic()
                                    .text_color(cx.theme().text_placeholder)
                                    .child(description),
                            )
                        }),
                )
        });
    }

    fn login_with_keys(&mut self, password: String, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.key_input.read(cx).value().to_string();
        let secret_key = if value.starts_with("nsec1") {
            SecretKey::parse(&value).ok()
        } else if value.starts_with("ncryptsec1") {
            EncryptedSecretKey::from_bech32(&value)
                .map(|enc| enc.decrypt(&password).ok())
                .unwrap_or_default()
        } else {
            None
        };

        if let Some(secret_key) = secret_key {
            // Active signer is no longer needed
            self.shutdown_active_signer(cx);

            let keys = Keys::new(secret_key);

            Identity::global(cx).update(cx, |this, cx| {
                this.write_keys(&keys, password, cx);
                this.set_signer(keys, window, cx);
            });
        } else {
            self.set_error("Secret Key is invalid", cx);
        }
    }

    fn login_with_bunker(&mut self, content: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(uri) = NostrConnectURI::parse(content) else {
            self.set_error("Bunker URL is not valid", cx);
            return;
        };

        let client_keys = ClientKeys::get_global(cx).keys();
        let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT / 2);

        let Ok(mut signer) = NostrConnect::new(uri, client_keys, timeout, None) else {
            self.set_error("Failed to create remote signer", cx);
            return;
        };

        // Active signer is no longer needed
        self.shutdown_active_signer(cx);

        // Automatically open auth url
        signer.auth_url_handler(CoopAuthUrlHandler);

        let (tx, rx) = oneshot::channel::<Option<(NostrConnect, NostrConnectURI)>>();

        // Verify remote signer connection
        cx.background_spawn(async move {
            if let Ok(bunker_uri) = signer.bunker_uri().await {
                tx.send(Some((signer, bunker_uri))).ok();
            } else {
                tx.send(None).ok();
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Some((signer, uri))) = rx.await {
                cx.update(|window, cx| {
                    Identity::global(cx).update(cx, |this, cx| {
                        this.write_bunker(&uri, cx);
                        this.set_signer(signer, window, cx);
                    });
                })
                .ok();
            } else {
                this.update(cx, |this, cx| {
                    let msg = "Connection to the Remote Signer failed or timed out";
                    this.set_error(msg, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn wait_for_connection(
        &mut self,
        signer: NostrConnect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) = oneshot::channel::<Option<(NostrConnectURI, NostrConnect)>>();

        cx.background_spawn(async move {
            if let Ok(bunker_uri) = signer.bunker_uri().await {
                tx.send(Some((bunker_uri, signer))).ok();
            } else {
                tx.send(None).ok();
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Some((uri, signer))) = rx.await {
                cx.update(|window, cx| {
                    Identity::global(cx).update(cx, |this, cx| {
                        this.write_bunker(&uri, cx);
                        this.set_signer(signer, window, cx);
                    });
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    window.push_notification(Notification::error("Connection failed"), cx);
                    // Refresh the active signer
                    this.update(cx, |this, cx| {
                        this.change_relay(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn change_relay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(relay_url) = RelayUrl::parse(self.relay_input.read(cx).value().to_string().as_str())
        else {
            window.push_notification(Notification::error("Relay URL is not valid."), cx);
            return;
        };

        let client_keys = ClientKeys::get_global(cx).keys();
        let uri = NostrConnectURI::client(client_keys.public_key(), vec![relay_url], "Coop");

        self.connection_string.update(cx, |this, cx| {
            *this = uri;
            cx.notify();
        });
    }

    fn shutdown_active_signer(&self, cx: &Context<Self>) {
        if let Some(signer) = self.active_signer.read(cx).clone() {
            cx.background_spawn(async move {
                signer.shutdown().await;
            })
            .detach();
        }
    }

    fn set_error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.set_logging_in(false, cx);
        self.error.update(cx, |this, cx| {
            *this = Some(message.into());
            cx.notify();
        });

        // Clear the error message after 3 secs
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;

            this.update(cx, |this, cx| {
                this.error.update(cx, |this, cx| {
                    *this = None;
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    fn set_logging_in(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_logging_in = status;
        cx.notify();
    }
}

impl Panel for Login {
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

impl EventEmitter<PanelEvent> for Login {}

impl Focusable for Login {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Login {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .flex()
            .child(
                div()
                    .h_full()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w_80()
                            .flex()
                            .flex_col()
                            .gap_8()
                            .child(
                                div()
                                    .text_center()
                                    .child(
                                        div()
                                            .text_center()
                                            .text_xl()
                                            .font_semibold()
                                            .line_height(relative(1.3))
                                            .child("Welcome Back!"),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_muted)
                                            .child("Continue with Private Key or Bunker"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(TextInput::new(&self.key_input))
                                    .child(
                                        Button::new("login")
                                            .label("Continue")
                                            .primary()
                                            .loading(self.is_logging_in)
                                            .disabled(self.is_logging_in)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.login(window, cx);
                                            })),
                                    )
                                    .when_some(self.error.read(cx).clone(), |this, error| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_center()
                                                .text_color(red())
                                                .child(error),
                                        )
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .h_full()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().surface_background)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .text_center()
                            .child(
                                div()
                                    .text_center()
                                    .child(
                                        div()
                                            .font_semibold()
                                            .line_height(relative(1.2))
                                            .text_color(cx.theme().text)
                                            .child("Continue with Nostr Connect"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().text_muted)
                                            .child("Use Nostr Connect apps to scan the code"),
                                    ),
                            )
                            .when_some(self.qr_image.read(cx).clone(), |this, qr| {
                                this.child(
                                    div()
                                        .id("")
                                        .mb_2()
                                        .p_2()
                                        .size_72()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap_2()
                                        .rounded_2xl()
                                        .shadow_md()
                                        .when(cx.theme().mode.is_dark(), |this| {
                                            this.shadow_none()
                                                .border_1()
                                                .border_color(cx.theme().border)
                                        })
                                        .bg(cx.theme().background)
                                        .child(img(qr).h_64())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            #[cfg(any(
                                                target_os = "linux",
                                                target_os = "freebsd"
                                            ))]
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                this.connection_string.read(cx).to_string(),
                                            ));
                                            #[cfg(any(
                                                target_os = "macos",
                                                target_os = "windows"
                                            ))]
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                this.connection_string.read(cx).to_string(),
                                            ));
                                            window.push_notification(
                                                "Connection String has been copied",
                                                cx,
                                            );
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_1()
                                    .child(TextInput::new(&self.relay_input).xsmall())
                                    .child(
                                        Button::new("change")
                                            .label("Change")
                                            .ghost()
                                            .xsmall()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.change_relay(window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}
