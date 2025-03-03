use gpui::{
    div, prelude::FluentBuilder, relative, svg, App, AppContext, Context, Entity, IntoElement,
    ParentElement, Render, Styled, Window,
};
use nostr_connect::prelude::*;
use std::time::Duration;
use ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    theme::{scale::ColorScaleStep, ActiveTheme},
    Disableable, Root, Size, StyledExt,
};

use super::app;
use crate::device;

const LOGO_URL: &str = "brand/coop.svg";
const TITLE: &str = "Welcome to Coop!";
const SUBTITLE: &str = "A Nostr client for secure communication.";
const JOIN_URL: &str = "https://start.njump.me/";
const ALPHA_MESSAGE: &str =
    "Coop is in the alpha stage of development; It may contain bugs, unfinished features, or unexpected behavior.";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

pub struct Onboarding {
    bunker_input: Entity<TextInput>,
    open_connect: bool,
    is_loading: bool,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let bunker_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::XSmall)
                .placeholder("bunker://<pubkey>?relay=wss://relay.example.com")
        });

        cx.new(|cx| {
            let mut subscriptions = vec![];

            subscriptions.push(cx.subscribe_in(
                &bunker_input,
                window,
                move |this: &mut Self, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.login(window, cx);
                    }
                },
            ));

            Self {
                bunker_input,
                open_connect: false,
                is_loading: false,
            }
        })
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.bunker_input.read(cx).text().to_string();
        let handle = window.window_handle();
        let keys = Keys::generate();

        // Show loading spinner
        self.set_loading(true, cx);

        let Ok(uri) = NostrConnectURI::parse(text) else {
            self.set_loading(false, cx);
            // TODO: handle error
            return;
        };

        let Ok(signer) = NostrConnect::new(uri, keys, Duration::from_secs(300), None) else {
            self.set_loading(false, cx);
            // TODO: handle error
            return;
        };

        cx.spawn(|_, cx| async move {
            if device::init(signer, &cx).await.is_ok() {
                _ = cx.update(|cx| {
                    handle
                        .update(cx, |_, window, cx| {
                            window.replace_root(cx, |window, cx| {
                                Root::new(app::init(window, cx).into(), window, cx)
                            });
                        })
                        .ok();
                });
            } else {
                // TODO: handle error
            }
        })
        .detach();
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }

    fn open_connect(&mut self, open: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_connect = open;
        cx.notify();
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(div().w_72().map(|this| {
                        if self.open_connect {
                            this.w_full()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .text_xs()
                                        .child("Bunker URI:")
                                        .child(self.bunker_input.clone()),
                                )
                                .child(
                                    Button::new("login")
                                        .label("Login")
                                        .primary()
                                        .w_full()
                                        .loading(self.is_loading)
                                        .disabled(self.is_loading)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.login(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("cancel")
                                        .label("Cancel")
                                        .ghost()
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_connect(false, window, cx);
                                        })),
                                )
                        } else {
                            this.w_full()
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
                                            this.open_connect(true, window, cx);
                                        })),
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
                    })),
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
