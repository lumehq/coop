use std::{sync::Arc, time::Duration};

use account::Account;
use common::create_qr;
use global::get_client_keys;
use gpui::{
    div, img, prelude::FluentBuilder, relative, AnyElement, App, AppContext, Context, Entity,
    EventEmitter, FocusHandle, Focusable, Image, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use ui::{
    button::{Button, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    input::{InputEvent, TextInput},
    notification::Notification,
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Disableable, Sizable, Size, StyledExt,
};

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
    // Inputs
    key_input: Entity<TextInput>,
    error: Entity<Option<SharedString>>,
    is_logging_in: bool,
    // Nostr Connect
    qr: Entity<Option<Arc<Image>>>,
    connect_relay: Entity<TextInput>,
    connect_client: Entity<Option<NostrConnectURI>>,
    // Keep track of all signers created by nostr connect
    signers: SmallVec<[NostrConnect; 3]>,
    // Panel
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 4]>,
}

impl Login {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let connect_client: Entity<Option<NostrConnectURI>> = cx.new(|_| None);
        let error = cx.new(|_| None);
        let qr = cx.new(|_| None);

        let signers = smallvec![];
        let mut subscriptions = smallvec![];

        let key_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::Small)
                .placeholder("nsec... or bunker://...")
        });

        let connect_relay = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).text_size(Size::XSmall).small();
            input.set_text("wss://relay.nsec.app", window, cx);
            input
        });

        subscriptions.push(cx.subscribe_in(
            &key_input,
            window,
            move |this, _, event, window, cx| {
                if let InputEvent::PressEnter = event {
                    this.login(window, cx);
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            &connect_relay,
            window,
            move |this, _, event, window, cx| {
                if let InputEvent::PressEnter = event {
                    this.change_relay(window, cx);
                }
            },
        ));

        subscriptions.push(
            cx.observe_in(&connect_client, window, |this, uri, window, cx| {
                let keys = get_client_keys().to_owned();

                if let Some(uri) = uri.read(cx).clone() {
                    if let Ok(qr) = create_qr(uri.to_string().as_str()) {
                        this.qr.update(cx, |this, cx| {
                            *this = Some(qr);
                            cx.notify();
                        });
                    }

                    // Shutdown all previous nostr connect clients
                    for client in std::mem::take(&mut this.signers).into_iter() {
                        cx.background_spawn(async move {
                            client.shutdown().await;
                        })
                        .detach();
                    }

                    // Create a new nostr connect client
                    match NostrConnect::new(uri, keys, Duration::from_secs(200), None) {
                        Ok(mut signer) => {
                            // Handle auth url
                            signer.auth_url_handler(CoopAuthUrlHandler);
                            // Store this signer for further clean up
                            this.signers.push(signer.clone());

                            Account::global(cx).update(cx, |this, cx| {
                                this.login(signer, window, cx);
                            });
                        }
                        Err(e) => {
                            window.push_notification(Notification::error(e.to_string()), cx);
                        }
                    }
                }
            }),
        );

        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;

            cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.change_relay(window, cx);
                })
                .ok();
            })
            .ok();
        })
        .detach();

        Self {
            key_input,
            connect_relay,
            connect_client,
            subscriptions,
            signers,
            error,
            qr,
            is_logging_in: false,
            name: "Login".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_logging_in {
            return;
        };

        self.set_logging_in(true, cx);

        let content = self.key_input.read(cx).text();
        let account = Account::global(cx);

        if content.starts_with("nsec1") {
            match SecretKey::parse(content.as_ref()) {
                Ok(secret) => {
                    let keys = Keys::new(secret);

                    account.update(cx, |this, cx| {
                        this.login(keys, window, cx);
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

            self.connect_client.update(cx, |this, cx| {
                *this = Some(uri);
                cx.notify();
            });
        } else {
            self.set_error("You must provide a valid Private Key or Bunker.".into(), cx);
        };
    }

    fn change_relay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(relay_url) =
            RelayUrl::parse(self.connect_relay.read(cx).text().to_string().as_str())
        else {
            window.push_notification(Notification::error("Relay URL is not valid."), cx);
            return;
        };

        let client_pubkey = get_client_keys().public_key();
        let uri = NostrConnectURI::client(client_pubkey, vec![relay_url], "Coop");

        self.connect_client.update(cx, |this, cx| {
            *this = Some(uri);
            cx.notify();
        });
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

    fn closable(&self, _cx: &App) -> bool {
        self.closable
    }

    fn zoomable(&self, _cx: &App) -> bool {
        self.zoomable
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
                                            .text_color(
                                                cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
                                            )
                                            .child("Continue with Private Key or Bunker"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(self.key_input.clone())
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
                                                .text_color(cx.theme().danger)
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
                    .bg(cx.theme().base.step(cx, ColorScaleStep::TWO))
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
                                            .text_color(
                                                cx.theme().base.step(cx, ColorScaleStep::TWELVE),
                                            )
                                            .child("Continue with Nostr Connect"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(
                                                cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
                                            )
                                            .child("Use Nostr Connect apps to scan the code"),
                                    ),
                            )
                            .when_some(self.qr.read(cx).clone(), |this, qr| {
                                this.child(
                                    div()
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
                                        .when(cx.theme().appearance.is_dark(), |this| {
                                            this.shadow_none().border_1().border_color(
                                                cx.theme().base.step(cx, ColorScaleStep::SIX),
                                            )
                                        })
                                        .bg(cx.theme().background)
                                        .child(img(qr).h_56()),
                                )
                            })
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_1()
                                    .child(self.connect_relay.clone())
                                    .child(
                                        Button::new("change")
                                            .label("Change")
                                            .ghost()
                                            .small()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.change_relay(window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}
