use common::qr::create_qr;
use gpui::{
    div, img, prelude::FluentBuilder, relative, svg, AnyElement, App, AppContext, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use std::{path::PathBuf, sync::Arc, time::Duration};
use ui::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    input::{InputEvent, TextInput},
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    Disableable, Size, StyledExt,
};

use crate::device::Device;

const LOGO_URL: &str = "brand/coop.svg";
const TITLE: &str = "Welcome to Coop!";
const SUBTITLE: &str = "A Nostr client for secure communication.";
// TODO: Replace it with Persona Mobile App
const NSTART_URL: &str =
    "https://start.njump.me?an=Coop&at=ios&ac=coop&afb=yes&asf=yes&aan=null&aac=null&arr=wss://relay.damus.io&awr=wss://relay.primal.net,wss://purplerelay.com,wss://offchain.pub";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

enum PageKind {
    Bunker,
    Connect,
    Selection,
}

pub struct Onboarding {
    // Panel
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Onboarding
    bunker_input: Entity<TextInput>,
    connect_url: Entity<Option<PathBuf>>,
    error_message: Entity<Option<SharedString>>,
    open: PageKind,
    is_loading: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let connect_url = cx.new(|_| None);
        let error_message = cx.new(|_| None);
        let bunker_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::XSmall)
                .placeholder("bunker://<pubkey>?relay=wss://relay.example.com")
        });

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.subscribe_in(
                &bunker_input,
                window,
                move |this: &mut Self, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.connect(window, cx);
                    }
                },
            ));

            Self {
                name: "Onboarding".into(),
                closable: true,
                zoomable: true,
                focus_handle: cx.focus_handle(),
                bunker_input,
                connect_url,
                error_message,
                subscriptions,
                open: PageKind::Selection,
                is_loading: false,
            }
        })
    }

    fn login(&self, signer: NostrConnect, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(device) = Device::global(cx) else {
            return;
        };

        let entity = cx.weak_entity();

        device.update(cx, |this, cx| {
            let login = this.login(signer, cx);

            cx.spawn(|_, cx| async move {
                if let Err(e) = login.await {
                    cx.update(|cx| {
                        entity
                            .update(cx, |this, cx| {
                                this.set_error(e.to_string(), cx);
                                this.set_loading(false, cx);
                            })
                            .ok();
                    })
                    .ok();
                }
            })
            .detach();
        });
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(model) = Device::global(cx) else {
            return;
        };

        let text = self.bunker_input.read(cx).text().to_string();
        let keys = Arc::unwrap_or_clone(model.read(cx).client_keys());

        self.set_loading(true, cx);

        let Ok(uri) = NostrConnectURI::parse(text) else {
            self.set_loading(false, cx);
            self.set_error("Bunker URL is invalid".to_owned(), cx);
            return;
        };

        let Ok(signer) = NostrConnect::new(uri, keys, Duration::from_secs(300), None) else {
            self.set_loading(false, cx);
            self.set_error("Failed to establish connection".to_owned(), cx);
            return;
        };

        self.login(signer, window, cx);
    }

    fn wait_for_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app_keys = Keys::generate();
        let url = NostrConnectURI::client(
            app_keys.public_key(),
            vec![RelayUrl::parse("wss://relay.nsec.app").unwrap()],
            "Coop",
        );

        // Create QR code and save it to a app directory
        let qr_path = create_qr(url.to_string().as_str()).ok();

        // Update QR code path
        self.connect_url.update(cx, |this, cx| {
            *this = qr_path;
            cx.notify();
        });

        // Open Connect page
        self.open(PageKind::Connect, window, cx);

        // Wait for connection
        if let Ok(signer) = NostrConnect::new(url, app_keys, Duration::from_secs(300), None) {
            self.login(signer, window, cx);
        } else {
            self.set_loading(false, cx);
            self.set_error("Failed to establish connection".to_owned(), cx);
            self.open(PageKind::Selection, window, cx);
        }
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }

    fn set_error(&mut self, msg: String, cx: &mut Context<Self>) {
        self.error_message.update(cx, |this, cx| {
            *this = Some(msg.into());
            cx.notify();
        });

        // Dismiss error message after 3 seconds
        cx.spawn(|this, cx| async move {
            cx.background_executor().timer(Duration::from_secs(3)).await;

            _ = cx.update(|cx| {
                this.update(cx, |this, cx| {
                    this.error_message.update(cx, |this, cx| {
                        *this = None;
                        cx.notify();
                    })
                })
            });
        })
        .detach();
    }

    fn open(&mut self, kind: PageKind, _window: &mut Window, cx: &mut Context<Self>) {
        self.open = kind;
        cx.notify();
    }
}

impl Panel for Onboarding {
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

impl EventEmitter<PanelEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_8()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_4()
                            .child(
                                svg()
                                    .path(LOGO_URL)
                                    .size_12()
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                            )
                            .child(
                                div()
                                    .text_center()
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_semibold()
                                            .line_height(relative(1.2))
                                            .child(TITLE),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(
                                                cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
                                            )
                                            .child(SUBTITLE),
                                    ),
                            ),
                    )
                    .child(div().w_72().w_full().flex().flex_col().gap_2().map(|this| {
                        match self.open {
                            PageKind::Connect => this
                                .when_some(self.connect_url.read(cx).as_ref(), |this, path| {
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
                                            .rounded_lg()
                                            .shadow_md()
                                            .when(cx.theme().appearance.is_dark(), |this| {
                                                this.shadow_none().border_1().border_color(
                                                    cx.theme().base.step(cx, ColorScaleStep::SIX),
                                                )
                                            })
                                            .bg(cx.theme().background)
                                            .child(img(path.as_path()).h_64()),
                                    )
                                })
                                .child(
                                    div()
                                        .text_xs()
                                        .text_center()
                                        .font_semibold()
                                        .line_height(relative(1.2))
                                        .child("Scan this QR to connect"),
                                )
                                .child(
                                    Button::new("wait_for_connection")
                                        .label("Waiting for connection")
                                        .primary()
                                        .w_full()
                                        .loading(true)
                                        .disabled(true),
                                )
                                .child(
                                    Button::new("use_url")
                                        .label("Use Bunker URL")
                                        .custom(
                                            ButtonCustomVariant::new(window, cx)
                                                .color(
                                                    cx.theme().base.step(cx, ColorScaleStep::THREE),
                                                )
                                                .border(
                                                    cx.theme().base.step(cx, ColorScaleStep::THREE),
                                                )
                                                .hover(
                                                    cx.theme().base.step(cx, ColorScaleStep::FOUR),
                                                )
                                                .active(
                                                    cx.theme().base.step(cx, ColorScaleStep::FIVE),
                                                )
                                                .foreground(
                                                    cx.theme()
                                                        .base
                                                        .step(cx, ColorScaleStep::TWELVE),
                                                ),
                                        )
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open(PageKind::Bunker, window, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .my_2()
                                        .w_full()
                                        .h_px()
                                        .bg(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                                )
                                .child(
                                    Button::new("cancel")
                                        .label("Cancel")
                                        .ghost()
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open(PageKind::Selection, window, cx);
                                        })),
                                ),
                            PageKind::Bunker => this
                                .child(
                                    div()
                                        .mb_2()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .text_xs()
                                        .child("Bunker URL:")
                                        .child(self.bunker_input.clone())
                                        .when_some(
                                            self.error_message.read(cx).as_ref(),
                                            |this, error| {
                                                this.child(
                                                    div()
                                                        .my_1()
                                                        .text_xs()
                                                        .text_center()
                                                        .text_color(cx.theme().danger)
                                                        .child(error.clone()),
                                                )
                                            },
                                        ),
                                )
                                .child(
                                    Button::new("login")
                                        .label("Login")
                                        .primary()
                                        .w_full()
                                        .loading(self.is_loading)
                                        .disabled(self.is_loading)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.connect(window, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .my_2()
                                        .w_full()
                                        .h_px()
                                        .bg(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                                )
                                .child(
                                    Button::new("cancel")
                                        .label("Cancel")
                                        .ghost()
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open(PageKind::Selection, window, cx);
                                        })),
                                ),
                            PageKind::Selection => this
                                .child(
                                    Button::new("login_connect_btn")
                                        .label("Login with Nostr Connect")
                                        .primary()
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.wait_for_connection(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("join_btn")
                                        .label("Are you new? Join here!")
                                        .ghost()
                                        .w_full()
                                        .on_click(|_, _, cx| {
                                            cx.open_url(NSTART_URL);
                                        }),
                                ),
                        }
                    })),
            )
    }
}
