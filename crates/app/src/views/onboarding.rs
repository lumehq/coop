use account::login;
use common::qr::create_qr;
use gpui::{
    div, img, prelude::FluentBuilder, relative, svg, App, AppContext, ClipboardItem, Context, Div,
    Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use nostr_connect::prelude::*;
use std::{path::PathBuf, sync::Arc, time::Duration};
use ui::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{InputEvent, TextInput},
    notification::NotificationType,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Root, Size, StyledExt,
};

use super::app;

const LOGO_URL: &str = "brand/coop.svg";
const TITLE: &str = "Welcome to Coop!";
const SUBTITLE: &str = "A Nostr client for secure communication.";
const ALPHA_MESSAGE: &str =
    "Coop is in the alpha stage of development; It may contain bugs, unfinished features, or unexpected behavior.";

const JOIN_URL: &str = "https://start.njump.me/";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

pub struct Onboarding {
    app_keys: Keys,
    connect_uri: NostrConnectURI,
    qr_path: Option<PathBuf>,
    nsec_input: Entity<TextInput>,
    use_connect: bool,
    use_privkey: bool,
    is_loading: bool,
    #[allow(dead_code)]
    subscriptions: Vec<Subscription>,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let app_keys = Keys::generate();

        let connect_uri = NostrConnectURI::client(
            app_keys.public_key(),
            vec![RelayUrl::parse("wss://relay.nsec.app").unwrap()],
            "Coop",
        );

        let nsec_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::XSmall)
                .placeholder("nsec...")
        });

        // Save Connect URI as PNG file for display as QR Code
        let qr_path = create_qr(connect_uri.to_string().as_str()).ok();

        cx.new(|cx| {
            // Handle Enter event for nsec input
            let subscriptions = vec![cx.subscribe_in(
                &nsec_input,
                window,
                move |this: &mut Self, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.login_with_private_key(window, cx);
                    }
                },
            )];

            Self {
                app_keys,
                connect_uri,
                qr_path,
                nsec_input,
                use_connect: false,
                use_privkey: false,
                is_loading: false,
                subscriptions,
            }
        })
    }

    fn login_with_nostr_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let uri = self.connect_uri.clone();
        let app_keys = self.app_keys.clone();
        let window_handle = window.window_handle();

        // Show QR Code for login with Nostr Connect
        self.use_connect(window, cx);

        // Wait for connection
        let (tx, rx) = oneshot::channel::<NostrConnect>();

        cx.background_spawn(async move {
            if let Ok(signer) = NostrConnect::new(uri, app_keys, Duration::from_secs(300), None) {
                tx.send(signer).ok();
            }
        })
        .detach();

        cx.spawn(|this, cx| async move {
            if let Ok(signer) = rx.await {
                cx.spawn(|mut cx| async move {
                    let signer = Arc::new(signer);

                    if login(signer, &cx).await.is_ok() {
                        _ = cx.update_window(window_handle, |_, window, cx| {
                            window.replace_root(cx, |window, cx| {
                                Root::new(app::init(window, cx).into(), window, cx)
                            });
                        })
                    }
                })
                .detach();
            } else {
                _ = cx.update(|cx| {
                    _ = this.update(cx, |this, cx| {
                        this.set_loading(false, cx);
                    });
                });
            }
        })
        .detach();
    }

    fn login_with_private_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.nsec_input.read(cx).text().to_string();
        let window_handle = window.window_handle();

        if !value.starts_with("nsec") || value.is_empty() {
            window.push_notification((NotificationType::Warning, "Private Key is required"), cx);
            return;
        }

        let keys = if let Ok(keys) = Keys::parse(&value) {
            keys
        } else {
            window.push_notification((NotificationType::Warning, "Private Key isn't valid"), cx);
            return;
        };

        // Show loading spinner
        self.set_loading(true, cx);

        cx.spawn(|this, mut cx| async move {
            let signer = Arc::new(keys);

            if login(signer, &cx).await.is_ok() {
                _ = cx.update_window(window_handle, |_, window, cx| {
                    window.replace_root(cx, |window, cx| {
                        Root::new(app::init(window, cx).into(), window, cx)
                    });
                })
            } else {
                _ = cx.update(|cx| {
                    _ = this.update(cx, |this, cx| {
                        this.set_loading(false, cx);
                    });
                });
            }
        })
        .detach();
    }

    fn use_connect(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.use_connect = true;
        cx.notify();
    }

    fn use_privkey(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.use_privkey = true;
        cx.notify();
    }

    fn reset(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.use_privkey = false;
        self.use_connect = false;
        cx.notify();
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }

    fn render_selection(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Button::new("login_connect_btn")
                    .label("Login with Nostr Connect")
                    .primary()
                    .w_full()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.login_with_nostr_connect(window, cx);
                    })),
            )
            .child(
                Button::new("login_privkey_btn")
                    .label("Login with Private Key")
                    .custom(
                        ButtonCustomVariant::new(window, cx)
                            .color(cx.theme().base.step(cx, ColorScaleStep::THREE))
                            .border(cx.theme().base.step(cx, ColorScaleStep::THREE))
                            .hover(cx.theme().base.step(cx, ColorScaleStep::FOUR))
                            .active(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                            .foreground(cx.theme().base.step(cx, ColorScaleStep::TWELVE)),
                    )
                    .w_full()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.use_privkey(window, cx);
                    })),
            )
            .child(
                div()
                    .my_2()
                    .h_px()
                    .rounded_md()
                    .w_full()
                    .bg(cx.theme().base.step(cx, ColorScaleStep::THREE)),
            )
            .child(
                Button::new("join_btn")
                    .label("Are you new? Join here!")
                    .ghost()
                    .w_full()
                    .on_click(|_, _, cx| {
                        cx.open_url(JOIN_URL);
                    }),
            )
    }

    fn render_connect_login(&self, cx: &mut Context<Self>) -> Div {
        let connect_string = self.connect_uri.to_string();

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .text_xs()
                    .text_center()
                    .child(
                        div()
                            .font_semibold()
                            .line_height(relative(1.2))
                            .child("Scan this QR Code in the Nostr Signer app"),
                    )
                    .child("Recommend: Amber (Android), nsec.app (web),..."),
            )
            .when_some(self.qr_path.clone(), |this, path| {
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
                        .shadow_lg()
                        .when(cx.theme().appearance.is_dark(), |this| {
                            this.shadow_none()
                                .border_1()
                                .border_color(cx.theme().base.step(cx, ColorScaleStep::SIX))
                        })
                        .bg(cx.theme().background)
                        .child(img(path).h_64()),
                )
            })
            .child(
                Button::new("copy")
                    .label("Copy Connection String")
                    .primary()
                    .w_full()
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(connect_string.clone()))
                    }),
            )
            .child(
                Button::new("cancel")
                    .label("Cancel")
                    .ghost()
                    .w_full()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reset(window, cx);
                    })),
            )
    }

    fn render_privkey_login(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .child("Private Key:")
                    .child(self.nsec_input.clone()),
            )
            .child(
                Button::new("login")
                    .label("Login")
                    .primary()
                    .w_full()
                    .loading(self.is_loading)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.login_with_private_key(window, cx);
                    })),
            )
            .child(
                Button::new("cancel")
                    .label("Cancel")
                    .ghost()
                    .w_full()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reset(window, cx);
                    })),
            )
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
                    .child(
                        div()
                            .w_72()
                            .map(|_| match (self.use_privkey, self.use_connect) {
                                (true, _) => self.render_privkey_login(cx),
                                (_, true) => self.render_connect_login(cx),
                                _ => self.render_selection(window, cx),
                            }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .bottom_2()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_center()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .child(ALPHA_MESSAGE),
            )
    }
}
