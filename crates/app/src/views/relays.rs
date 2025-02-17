use gpui::{
    div, prelude::FluentBuilder, px, uniform_list, AppContext, Context, Entity, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, TextAlign, Window,
};
use nostr_sdk::prelude::*;
use state::get_client;
use ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, IconName, Sizable,
};

const MESSAGE: &str = "In order to receive messages from others, you need to setup Messaging Relays. You can use the recommend relays or add more.";

pub struct Relays {
    relays: Entity<Vec<Url>>,
    input: Entity<TextInput>,
    focus_handle: FocusHandle,
    is_loading: bool,
}

impl Relays {
    pub fn new(
        relays: Option<Vec<String>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let relays = cx.new(|_| {
            if let Some(value) = relays {
                value.into_iter().map(|v| Url::parse(&v).unwrap()).collect()
            } else {
                vec![
                    Url::parse("wss://auth.nostr1.com").unwrap(),
                    Url::parse("wss://relay.0xchat.com").unwrap(),
                ]
            }
        });

        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(ui::Size::XSmall)
                .small()
                .placeholder("wss://...")
        });

        cx.subscribe_in(&input, window, move |this, _, input_event, window, cx| {
            if let InputEvent::PressEnter = input_event {
                this.add(window, cx);
            }
        })
        .detach();

        Self {
            relays,
            input,
            is_loading: false,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = self.relays.read(cx).clone();
        let window_handle = window.window_handle();

        self.set_loading(true, cx);

        let client = get_client();
        let (tx, rx) = oneshot::channel();

        cx.background_spawn(async move {
            let signer = client.signer().await.expect("Signer is required");
            let public_key = signer
                .get_public_key()
                .await
                .expect("Cannot get public key");

            // If user didn't have any NIP-65 relays, add default ones
            // TODO: Is this really necessary?
            if let Ok(relay_list) = client.database().relay_list(public_key).await {
                if relay_list.is_empty() {
                    let builder = EventBuilder::relay_list(vec![
                        (RelayUrl::parse("wss://relay.damus.io/").unwrap(), None),
                        (RelayUrl::parse("wss://relay.primal.net/").unwrap(), None),
                        (RelayUrl::parse("wss://nos.lol/").unwrap(), None),
                    ]);

                    if let Err(e) = client.send_event_builder(builder).await {
                        log::error!("Failed to send relay list event: {}", e)
                    }
                }
            }

            let tags: Vec<Tag> = relays
                .into_iter()
                .map(|relay| Tag::custom(TagKind::Relay, vec![relay.to_string()]))
                .collect();

            let builder = EventBuilder::new(Kind::InboxRelays, "").tags(tags);

            if let Ok(output) = client.send_event_builder(builder).await {
                _ = tx.send(output.val);
            };
        })
        .detach();

        cx.spawn(|this, mut cx| async move {
            if rx.await.is_ok() {
                _ = cx.update_window(window_handle, |_, window, cx| {
                    _ = this.update(cx, |this, cx| {
                        this.set_loading(false, cx);
                    });

                    window.close_modal(cx);
                });
            }
        })
        .detach();
    }

    pub fn loading(&self) -> bool {
        self.is_loading
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).text().to_string();

        if !value.starts_with("ws") {
            return;
        }

        if let Ok(url) = Url::parse(&value) {
            self.relays.update(cx, |this, cx| {
                if !this.contains(&url) {
                    this.push(url);
                    cx.notify();
                }
            });

            self.input.update(cx, |this, cx| {
                this.set_text("", window, cx);
            });
        }
    }

    fn remove(&mut self, ix: usize, _window: &mut Window, cx: &mut Context<Self>) {
        self.relays.update(cx, |this, cx| {
            this.remove(ix);
            cx.notify();
        });
    }
}

impl Render for Relays {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .child(MESSAGE),
            )
            .child(
                div()
                    .px_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.input.clone())
                            .child(
                                Button::new("add_relay_btn")
                                    .icon(IconName::Plus)
                                    .small()
                                    .rounded(px(cx.theme().radius))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.add(window, cx)),
                                    ),
                            ),
                    )
                    .map(|this| {
                        let view = cx.entity();
                        let relays = self.relays.read(cx).clone();
                        let total = relays.len();

                        if !relays.is_empty() {
                            this.child(
                                uniform_list(
                                    view,
                                    "relays",
                                    total,
                                    move |_, range, _window, cx| {
                                        let mut items = Vec::new();

                                        for ix in range {
                                            let item = relays.get(ix).unwrap().clone().to_string();

                                            items.push(
                                                div().group("").w_full().h_9().py_0p5().child(
                                                    div()
                                                        .px_2()
                                                        .h_full()
                                                        .w_full()
                                                        .flex()
                                                        .items_center()
                                                        .justify_between()
                                                        .rounded(px(cx.theme().radius))
                                                        .bg(cx
                                                            .theme()
                                                            .base
                                                            .step(cx, ColorScaleStep::THREE))
                                                        .text_xs()
                                                        .child(item)
                                                        .child(
                                                            Button::new("remove_{ix}")
                                                                .icon(IconName::Close)
                                                                .xsmall()
                                                                .ghost()
                                                                .invisible()
                                                                .group_hover("", |this| {
                                                                    this.visible()
                                                                })
                                                                .on_click(cx.listener(
                                                                    move |this, _, window, cx| {
                                                                        this.remove(ix, window, cx)
                                                                    },
                                                                )),
                                                        ),
                                                ),
                                            )
                                        }

                                        items
                                    },
                                )
                                .min_h(px(120.)),
                            )
                        } else {
                            this.h_20()
                                .mb_2()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_align(TextAlign::Center)
                                .child("Please add some relays.")
                        }
                    }),
            )
    }
}
