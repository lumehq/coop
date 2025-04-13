use anyhow::{anyhow, Error};
use global::{constants::NEW_MESSAGE_SUB_ID, get_client};
use gpui::{
    div, prelude::FluentBuilder, px, uniform_list, App, AppContext, Context, Entity, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Task, TextAlign,
    Window,
};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, IconName, Sizable,
};

const MESSAGE: &str = "In order to receive messages from others, you need to setup Messaging Relays. You can use the recommend relays or add more.";
const HELP_TEXT: &str = "Please add some relays.";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Relays> {
    Relays::new(window, cx)
}

pub struct Relays {
    relays: Entity<Vec<RelayUrl>>,
    input: Entity<TextInput>,
    focus_handle: FocusHandle,
    is_loading: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Relays {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let client = get_client();

        let relays = cx.new(|cx| {
            let relays = vec![
                RelayUrl::parse("wss://auth.nostr1.com").unwrap(),
                RelayUrl::parse("wss://relay.0xchat.com").unwrap(),
            ];

            let task: Task<Result<Vec<RelayUrl>, Error>> = cx.background_spawn(async move {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;

                let filter = Filter::new()
                    .kind(Kind::InboxRelays)
                    .author(public_key)
                    .limit(1);

                if let Some(event) = client.database().query(filter).await?.first_owned() {
                    let relays = event
                        .tags
                        .filter_standardized(TagKind::Relay)
                        .filter_map(|t| match t {
                            TagStandard::Relay(url) => Some(url.to_owned()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();

                    Ok(relays)
                } else {
                    Err(anyhow!("Messaging Relays not found."))
                }
            });

            cx.spawn(async move |this, cx| {
                if let Ok(relays) = task.await {
                    cx.update(|cx| {
                        this.update(cx, |this: &mut Vec<RelayUrl>, cx| {
                            *this = relays;
                            cx.notify();
                        })
                        .ok();
                    })
                    .ok();
                }
            })
            .detach();

            relays
        });

        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(ui::Size::XSmall)
                .small()
                .placeholder("wss://example.com")
        });

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Relays, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.add(window, cx);
                    }
                },
            ));

            Self {
                relays,
                input,
                subscriptions,
                is_loading: false,
                focus_handle: cx.focus_handle(),
            }
        })
    }

    pub fn update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Show loading spinner
        self.set_loading(true, cx);

        let relays = self.relays.read(cx).clone();

        let task: Task<Result<EventId, Error>> = cx.background_spawn(async move {
            let client = get_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // If user didn't have any NIP-65 relays, add default ones
            if client.database().relay_list(public_key).await?.is_empty() {
                let builder = EventBuilder::relay_list(vec![
                    (RelayUrl::parse("wss://relay.damus.io/").unwrap(), None),
                    (RelayUrl::parse("wss://relay.primal.net/").unwrap(), None),
                ]);

                if let Err(e) = client.send_event_builder(builder).await {
                    log::error!("Failed to send relay list event: {}", e);
                }
            }

            let tags: Vec<Tag> = relays
                .iter()
                .map(|relay| Tag::custom(TagKind::Relay, vec![relay.to_string()]))
                .collect();

            let builder = EventBuilder::new(Kind::InboxRelays, "").tags(tags);
            let output = client.send_event_builder(builder).await?;

            // Connect to messaging relays
            for relay in relays.into_iter() {
                _ = client.add_relay(&relay).await;
                _ = client.connect_relay(&relay).await;
            }

            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

            // Close old subscription
            client.unsubscribe(&sub_id).await;

            // Subscribe to new messages
            if let Err(e) = client
                .subscribe_with_id(
                    sub_id,
                    Filter::new()
                        .kind(Kind::GiftWrap)
                        .pubkey(public_key)
                        .limit(0),
                    None,
                )
                .await
            {
                log::error!("Failed to subscribe to new messages: {}", e);
            }

            Ok(output.val)
        });

        cx.spawn_in(window, async move |this, cx| {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_loading(false, cx);
                        cx.notify();
                    })
                    .ok();

                    window.close_modal(cx);
                })
                .ok();
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

        if let Ok(url) = RelayUrl::parse(&value) {
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
            .w_full()
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
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w_full()
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
                                .w_full()
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
                                .child(HELP_TEXT)
                        }
                    }),
            )
    }
}
