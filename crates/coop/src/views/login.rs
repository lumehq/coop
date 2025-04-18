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

const INPUT_INVALID: &str = "You must provide a valid Private Key or Bunker.";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Login> {
    Login::new(window, cx)
}

pub struct Login {
    // Inputs
    key_input: Entity<TextInput>,
    error_message: Entity<Option<SharedString>>,
    is_logging_in: bool,
    // Nostr Connect
    qr: Option<Arc<Image>>,
    connect_relay: Entity<TextInput>,
    connect_client: Entity<Option<NostrConnectURI>>,
    // Panel
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 3]>,
}

impl Login {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = smallvec![];
        let error_message = cx.new(|_| None);
        let connect_client = cx.new(|_: &mut Context<'_, Option<NostrConnectURI>>| None);

        let key_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::Small)
                .placeholder("nsec... or bunker://...")
        });

        let connect_relay = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).text_size(Size::Small).small();
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
                let account = Account::global(cx);

                if let Some(uri) = uri.read(cx).clone() {
                    if let Ok(qr) = create_qr(uri.to_string().as_str()) {
                        this.qr = Some(qr);
                        cx.notify();
                    }

                    match NostrConnect::new(uri, keys, Duration::from_secs(300), None) {
                        Ok(signer) => {
                            account.update(cx, |this, cx| {
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

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            cx.update(|cx| {
                this.update(cx, |this, cx| {
                    let Ok(relay_url) =
                        RelayUrl::parse(this.connect_relay.read(cx).text().to_string().as_str())
                    else {
                        return;
                    };

                    let client_pubkey = get_client_keys().public_key();
                    let uri = NostrConnectURI::client(client_pubkey, vec![relay_url], "Coop");

                    this.connect_client.update(cx, |this, cx| {
                        *this = Some(uri);
                        cx.notify();
                    });
                })
            })
            .ok();
        })
        .detach();

        Self {
            key_input,
            connect_relay,
            connect_client,
            subscriptions,
            error_message,
            qr: None,
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
                    self.set_error_message(e.to_string(), cx);
                    self.set_logging_in(false, cx);
                }
            }
        } else if content.starts_with("bunker://") {
            let keys = get_client_keys().to_owned();
            let Ok(uri) = NostrConnectURI::parse(content.as_ref()) else {
                self.set_error_message("Bunker URL is not valid".to_owned(), cx);
                self.set_logging_in(false, cx);
                return;
            };

            match NostrConnect::new(uri, keys, Duration::from_secs(120), None) {
                Ok(signer) => {
                    account.update(cx, |this, cx| {
                        this.login(signer, window, cx);
                    });
                }
                Err(e) => {
                    self.set_error_message(e.to_string(), cx);
                    self.set_logging_in(false, cx);
                }
            }
        } else {
            self.set_logging_in(false, cx);
            window.push_notification(Notification::error(INPUT_INVALID), cx);
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

    fn set_error_message(&mut self, message: String, cx: &mut Context<Self>) {
        self.error_message.update(cx, |this, cx| {
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
                                    .when_some(
                                        self.error_message.read(cx).clone(),
                                        |this, error| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .text_center()
                                                    .text_color(cx.theme().danger)
                                                    .child(error),
                                            )
                                        },
                                    ),
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
                            .when_some(self.qr.clone(), |this, qr| {
                                this.child(
                                    div()
                                        .mb_2()
                                        .p_2()
                                        .size_64()
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
