use std::sync::Arc;
use std::time::Duration;

use common::string_to_qr;
use global::constants::{APP_NAME, KEYRING_BUNKER, KEYRING_USER_PATH};
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, red, relative, AnyElement, App, AppContext, ClipboardItem, Context, Entity,
    EventEmitter, FocusHandle, Focusable, Image, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::{ContextModal, Disableable, Sizable, StyledExt};

const NOSTR_CONNECT_RELAY: &str = "wss://relay.nsec.app";
const NOSTR_CONNECT_TIMEOUT: u64 = 300;

#[derive(Debug, Clone)]
struct CoopAuthUrlHandler;

impl AuthUrlHandler for CoopAuthUrlHandler {
    fn on_auth_url(&self, auth_url: Url) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            webbrowser::open(auth_url.as_str())?;
            Ok(())
        })
    }
}

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
        let connection_string = cx.new(|_cx| {
            let relay = RelayUrl::parse(NOSTR_CONNECT_RELAY).unwrap();
            let client_keys = shared_state().client_signer.clone();

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

        subscriptions.push(
            cx.subscribe_in(&key_input, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.login(window, cx);
                }
            }),
        );

        subscriptions.push(
            cx.subscribe_in(&relay_input, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.change_relay(window, cx);
                }
            }),
        );

        subscriptions.push(cx.observe_new::<NostrConnectURI>(
            move |connection_string, _window, cx| {
                if let Ok(mut signer) = NostrConnect::new(
                    connection_string.to_owned(),
                    shared_state().client_signer.clone(),
                    Duration::from_secs(NOSTR_CONNECT_TIMEOUT),
                    None,
                ) {
                    // Automatically open remote signer's webpage when received auth url
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
                        *this = string_to_qr(&connection_string.to_string());
                        cx.notify();
                    })
                    .ok();
            },
        ));

        subscriptions.push(cx.observe_in(
            &connection_string,
            window,
            |this, entity, _window, cx| {
                let connection_string = entity.read(cx).clone();
                let client_keys = shared_state().client_signer.clone();

                // Update the QR Image with the new connection string
                this.qr_image.update(cx, |this, cx| {
                    *this = string_to_qr(&connection_string.to_string());
                    cx.notify();
                });

                if let Ok(mut signer) = NostrConnect::new(
                    connection_string,
                    client_keys,
                    Duration::from_secs(NOSTR_CONNECT_TIMEOUT),
                    None,
                ) {
                    // Automatically open remote signer's webpage when received auth url
                    signer.auth_url_handler(CoopAuthUrlHandler);

                    this.active_signer.update(cx, |this, cx| {
                        *this = Some(signer);
                        cx.notify();
                    });
                }
            },
        ));

        subscriptions.push(
            cx.observe_in(&active_signer, window, |_this, entity, window, cx| {
                if let Some(signer) = entity.read(cx).clone() {
                    let (tx, rx) = oneshot::channel::<NostrConnectURI>();

                    cx.background_spawn(async move {
                        if let Ok(bunker_uri) = signer.bunker_uri().await {
                            tx.send(bunker_uri).ok();

                            if let Err(e) = shared_state().set_signer(signer).await {
                                log::error!("{}", e);
                            }
                        }
                    })
                    .detach();

                    cx.spawn_in(window, async move |this, cx| {
                        if let Ok(uri) = rx.await {
                            this.update(cx, |this, cx| {
                                this.save_bunker(&uri, cx);
                            })
                            .ok();
                        }
                    })
                    .detach();
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

    fn login(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_logging_in {
            return;
        };
        self.set_logging_in(true, cx);

        let client_keys = shared_state().client_signer.clone();
        let content = self.key_input.read(cx).value();

        if content.starts_with("nsec1") {
            let Ok(keys) = SecretKey::parse(content.as_ref()).map(Keys::new) else {
                self.set_error("Secret key is not valid".to_owned(), cx);
                return;
            };

            // Active signer is no longer needed
            self.shutdown_active_signer(cx);

            // Save these keys to the OS storage for further logins
            self.save_keys(&keys, cx);

            // Set signer with this keys in the background
            cx.background_spawn(async move {
                if let Err(e) = shared_state().set_signer(keys).await {
                    log::error!("{}", e);
                }
            })
            .detach();
        } else if content.starts_with("bunker://") {
            let Ok(uri) = NostrConnectURI::parse(content.as_ref()) else {
                self.set_error("Bunker URL is not valid".to_owned(), cx);
                return;
            };

            // Active signer is no longer needed
            self.shutdown_active_signer(cx);

            // Save this bunker to the OS Storage for further logins
            self.save_bunker(&uri, cx);

            match NostrConnect::new(
                uri,
                client_keys,
                Duration::from_secs(NOSTR_CONNECT_TIMEOUT),
                None,
            ) {
                Ok(signer) => {
                    // Set signer with this remote signer in the background
                    cx.background_spawn(async move {
                        if let Err(e) = shared_state().set_signer(signer).await {
                            log::error!("{}", e);
                        }
                    })
                    .detach();
                }
                Err(e) => {
                    self.set_error(e.to_string(), cx);
                }
            }
        } else {
            self.set_error("You must provide a valid Private Key or Bunker.".into(), cx);
        };
    }

    fn change_relay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(relay_url) = RelayUrl::parse(self.relay_input.read(cx).value().to_string().as_str())
        else {
            window.push_notification(Notification::error("Relay URL is not valid."), cx);
            return;
        };

        let client_keys = shared_state().client_signer.clone();
        let uri = NostrConnectURI::client(client_keys.public_key(), vec![relay_url], "Coop");

        self.connection_string.update(cx, |this, cx| {
            *this = uri;
            cx.notify();
        });
    }

    fn save_keys(&self, keys: &Keys, cx: &mut Context<Self>) {
        let save_credential = cx.write_credentials(
            KEYRING_USER_PATH,
            keys.public_key().to_hex().as_str(),
            keys.secret_key().as_secret_bytes(),
        );

        cx.background_spawn(async move {
            if let Err(e) = save_credential.await {
                log::error!("Failed to save keys: {}", e)
            }
        })
        .detach();
    }

    fn save_bunker(&self, uri: &NostrConnectURI, cx: &mut Context<Self>) {
        let mut value = uri.to_string();

        // Remove the secret param if it exists
        if let Some(secret) = uri.secret() {
            value = value.replace(secret, "");
        }

        let save_credential =
            cx.write_credentials(KEYRING_USER_PATH, KEYRING_BUNKER, value.as_bytes());

        cx.background_spawn(async move {
            if let Err(e) = save_credential.await {
                log::error!("Failed to save the Bunker URI: {}", e)
            }
        })
        .detach();
    }

    fn shutdown_active_signer(&self, cx: &Context<Self>) {
        if let Some(signer) = self.active_signer.read(cx).clone() {
            cx.background_spawn(async move {
                signer.shutdown().await;
            })
            .detach();
        }
    }

    fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.set_logging_in(false, cx);
        self.error.update(cx, |this, cx| {
            *this = Some(SharedString::new(message));
            cx.notify();
        });
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
