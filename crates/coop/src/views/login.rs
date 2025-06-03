use std::sync::Arc;
use std::time::Duration;

use app_state::AppState;
use common::string_to_qr;
use global::{shared_state, NostrSignal};
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
        const NOSTR_CONNECT_RELAY: &str = "wss://relay.nsec.app";
        const NOSTR_CONNECT_TIMEOUT: u64 = 300;

        let error = cx.new(|_| None);
        let key_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("nsec... or bunker://..."));
        let relay_input =
            cx.new(|cx| InputState::new(window, cx).default_value(NOSTR_CONNECT_RELAY));
        let qr_image = cx.new(|_| None);
        let async_qr_image = qr_image.downgrade();
        let active_signer = cx.new(|_| None);
        let async_active_signer = active_signer.downgrade();

        let connection_string = cx.new(|cx| {
            let relay = RelayUrl::parse(NOSTR_CONNECT_RELAY).unwrap();
            let client_keys = AppState::get_global(cx)
                .client_keys()
                .cloned()
                .unwrap_or(Keys::generate());

            NostrConnectURI::client(client_keys.public_key(), vec![relay], "Coop")
        });

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
                // Update the QR Image with the new connection string
                async_qr_image
                    .update(cx, |this, cx| {
                        *this = string_to_qr(&connection_string.to_string());
                        cx.notify();
                    })
                    .ok();

                let client_keys = AppState::get_global(cx)
                    .client_keys()
                    .cloned()
                    .unwrap_or(Keys::generate());

                if let Ok(mut signer) = NostrConnect::new(
                    connection_string.to_owned(),
                    client_keys,
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
            },
        ));

        subscriptions.push(cx.observe_in(
            &connection_string,
            window,
            |this, entity, _window, cx| {
                let connection_string = entity.read(cx).clone();

                // Update the QR Image with the new connection string
                this.qr_image.update(cx, |this, cx| {
                    *this = string_to_qr(&connection_string.to_string());
                    cx.notify();
                });

                let client_keys = AppState::get_global(cx)
                    .client_keys()
                    .cloned()
                    .unwrap_or(Keys::generate());

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
            cx.observe_in(&active_signer, window, |_this, entity, _window, cx| {
                if let Some(signer) = entity.read(cx).clone() {
                    cx.background_spawn(async move {
                        if let Ok(bunker_uri) = signer.bunker_uri().await {
                            shared_state()
                                .global_sender
                                .send(NostrSignal::RemoteSigner((signer, bunker_uri)))
                                .await
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

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_logging_in {
            return;
        };
        self.set_logging_in(true, cx);

        let content = self.key_input.read(cx).value();

        if content.starts_with("nsec1") {
            match SecretKey::parse(content.as_ref()) {
                Ok(secret) => {
                    AppState::global(cx).update(cx, |this, cx| {
                        this.login(Keys::new(secret), window, cx);
                    });
                }
                Err(e) => {
                    self.set_error(e.to_string(), cx);
                }
            }
        } else if content.starts_with("bunker://") {
            let Ok(uri) = NostrConnectURI::parse(content.as_ref()) else {
                self.set_error("Bunker URL is not valid".to_owned(), cx);
                return;
            };

            // Active signer is no longer needed
            self.shutdown_active_signer(cx);

            let client_keys = AppState::get_global(cx)
                .client_keys()
                .cloned()
                .unwrap_or(Keys::generate());

            if let Ok(signer) = NostrConnect::new(uri, client_keys, Duration::from_secs(300), None)
            {
                AppState::global(cx).update(cx, |this, cx| {
                    this.login(signer, window, cx);
                });
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

        let client_keys = AppState::get_global(cx)
            .client_keys()
            .cloned()
            .unwrap_or(Keys::generate());

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
