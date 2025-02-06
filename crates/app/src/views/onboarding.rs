use app_state::registry::AppRegistry;
use common::profile::NostrProfile;
use gpui::{
    div, prelude::FluentBuilder, relative, svg, App, AppContext, BorrowAppContext, Context, Entity,
    IntoElement, ParentElement, Render, Styled, Window,
};
use nostr_connect::prelude::*;
use state::get_client;
use std::time::Duration;
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    notification::NotificationType,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Root, Size, StyledExt,
};

use super::app;

const ALPHA_MESSAGE: &str = "Coop is in the alpha stage; it does not store any credentials. You will need to log in again when you reopen the app.";
const JOIN_URL: &str = "https://start.njump.me/";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

pub struct Onboarding {
    input: Entity<TextInput>,
    use_connect: bool,
    use_privkey: bool,
    is_loading: bool,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::XSmall)
                .placeholder("nsec...")
        });

        cx.new(|cx| {
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.login(window, cx);
                    }
                },
            )
            .detach();

            Self {
                input,
                use_connect: false,
                use_privkey: false,
                is_loading: false,
            }
        })
    }

    /*
    fn use_connect(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.use_connect = true;
        cx.notify();
    }
    */

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

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).text().to_string();

        if !value.starts_with("nsec") || value.is_empty() {
            window.push_notification((NotificationType::Warning, "Private Key is required"), cx);
            return;
        }

        // Show loading spinner
        self.set_loading(true, cx);

        let window_handle = window.window_handle();
        let keys = if let Ok(keys) = Keys::parse(&value) {
            keys
        } else {
            // TODO: handle error
            return;
        };

        cx.spawn(|_, mut cx| async move {
            let client = get_client();
            let (tx, rx) = oneshot::channel::<NostrProfile>();

            cx.background_executor()
                .spawn(async move {
                    let public_key = keys.get_public_key().await.unwrap();
                    let metadata = client
                        .fetch_metadata(public_key, Duration::from_secs(3))
                        .await
                        .ok()
                        .unwrap_or(Metadata::new());
                    let profile = NostrProfile::new(public_key, metadata);

                    _ = tx.send(profile);
                    _ = client.set_signer(keys).await;
                })
                .detach();

            if let Ok(profile) = rx.await {
                cx.update_window(window_handle, |_, window, cx| {
                    cx.update_global::<AppRegistry, _>(|this, cx| {
                        this.set_user(Some(profile.clone()));

                        if let Some(root) = this.root() {
                            cx.update_entity(&root, |this: &mut Root, cx| {
                                this.set_view(app::init(profile, window, cx).into(), cx);
                            });
                        }
                    });
                })
                .unwrap();
            }
        })
        .detach();
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
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_4()
                            .child(
                                svg()
                                    .path("brand/coop.svg")
                                    .size_12()
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                            )
                            .child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_semibold()
                                            .line_height(relative(1.2))
                                            .child("Welcome to Coop!"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(
                                                cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
                                            )
                                            .child("A Nostr client for secure communication."),
                                    ),
                            ),
                    )
                    .child(div().w_72().map(|this| {
                        if self.use_privkey {
                            this.flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .text_xs()
                                        .child("Private Key:")
                                        .child(self.input.clone()),
                                )
                                .child(
                                    Button::new("login")
                                        .label("Login")
                                        .primary()
                                        .w_full()
                                        .loading(self.is_loading)
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
                                            this.reset(window, cx);
                                        })),
                                )
                        } else {
                            this.flex()
                                .flex_col()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("login_btn")
                                        .label("Login with Private Key")
                                        .primary()
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.use_privkey(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("join_btn")
                                        .label("Join Nostr")
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
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .text_align(gpui::TextAlign::Center)
                    .child(ALPHA_MESSAGE),
            )
    }
}
