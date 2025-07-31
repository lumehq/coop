use anyhow::Error;
use global::constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID};
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, uniform_list, App, AppContext, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Task, TextAlign, UniformList,
    Window,
};
use i18n::t;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::{h_flex, v_flex, IconName, Sizable};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Relays> {
    Relays::new(window, cx)
}

pub struct Relays {
    relays: Entity<Vec<RelayUrl>>,
    input: Entity<InputState>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Relays {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("wss://example.com"));
        let relays = cx.new(|cx| {
            let task: Task<Result<Vec<RelayUrl>, Error>> = cx.background_spawn(async move {
                let client = nostr_client();
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let filter = Filter::new()
                    .kind(Kind::InboxRelays)
                    .author(public_key)
                    .limit(1);

                if let Some(event) = client.database().query(filter).await?.first_owned() {
                    let relays = event
                        .tags
                        .filter(TagKind::Relay)
                        .filter_map(|tag| RelayUrl::parse(tag.content()?).ok())
                        .collect::<Vec<_>>();

                    Ok(relays)
                } else {
                    let relays = vec![
                        RelayUrl::parse("wss://auth.nostr1.com")?,
                        RelayUrl::parse("wss://relay.0xchat.com")?,
                    ];

                    Ok(relays)
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

            vec![]
        });

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Relays, _, event, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.add(window, cx);
                    }
                },
            ));

            Self {
                relays,
                input,
                subscriptions,
            }
        })
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();

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
                this.set_value("", window, cx);
            });
        }
    }

    fn remove(&mut self, ix: usize, _window: &mut Window, cx: &mut Context<Self>) {
        self.relays.update(cx, |this, cx| {
            this.remove(ix);
            cx.notify();
        });
    }

    pub fn set_relays(&mut self, cx: &mut Context<Self>) -> Task<Result<(), Error>> {
        let relays = self.relays.read(cx).clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // If user didn't have any NIP-65 relays, add default ones
            if client.database().relay_list(public_key).await?.is_empty() {
                let builder = EventBuilder::relay_list(vec![
                    (RelayUrl::parse("wss://relay.damus.io/").unwrap(), None),
                    (RelayUrl::parse("wss://relay.primal.net/").unwrap(), None),
                    (RelayUrl::parse("wss://nos.lol/").unwrap(), None),
                    (RelayUrl::parse("wss://relay.nostr.net/").unwrap(), None),
                ]);

                client.send_event_builder(builder).await?;
            }

            let tags = relays
                .iter()
                .map(|relay| Tag::relay(relay.clone()))
                .collect_vec();

            // Send event to update inbox relays
            client
                .send_event_builder(EventBuilder::new(Kind::InboxRelays, "").tags(tags))
                .await?;

            // Connect to messaging relays
            for relay in relays.into_iter() {
                _ = client.add_relay(&relay).await;
                _ = client.connect_relay(&relay).await;
            }

            let all_msg_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            let new_msg_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

            let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
            let new_messages = Filter::new()
                .kind(Kind::GiftWrap)
                .pubkey(public_key)
                .limit(0);

            // Close old subscriptions
            client.unsubscribe(&all_msg_id).await;
            client.unsubscribe(&new_msg_id).await;

            // Subscribe to all messages
            client
                .subscribe_with_id(all_msg_id, all_messages, None)
                .await?;

            // Subscribe to new messages
            client
                .subscribe_with_id(new_msg_id, new_messages, None)
                .await?;

            Ok(())
        })
    }

    fn render_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> UniformList {
        let relays = self.relays.read(cx).clone();
        let total = relays.len();

        uniform_list(
            "relays",
            total,
            cx.processor(move |_, range, _window, cx| {
                let mut items = Vec::new();

                for ix in range {
                    let item = relays.get(ix).map(|i: &RelayUrl| i.to_string()).unwrap();

                    items.push(
                        div().group("").w_full().h_9().py_0p5().child(
                            div()
                                .px_2()
                                .h_full()
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_between()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().elevated_surface_background)
                                .text_xs()
                                .child(item)
                                .child(
                                    Button::new("remove_{ix}")
                                        .icon(IconName::Close)
                                        .xsmall()
                                        .ghost()
                                        .invisible()
                                        .group_hover("", |this| this.visible())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.remove(ix, window, cx)
                                        })),
                                ),
                        ),
                    )
                }

                items
            }),
        )
        .w_full()
        .min_h(px(200.))
    }

    fn render_empty(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h_20()
            .mb_2()
            .justify_center()
            .text_sm()
            .text_align(TextAlign::Center)
            .child(SharedString::new(t!("relays.add_some_relays")))
    }
}

impl Render for Relays {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(SharedString::new(t!("relays.description"))),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .w_full()
                            .child(TextInput::new(&self.input).small())
                            .child(
                                Button::new("add_relay_btn")
                                    .icon(IconName::Plus)
                                    .label(t!("common.add"))
                                    .small()
                                    .ghost()
                                    .rounded_md()
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.add(window, cx)),
                                    ),
                            ),
                    )
                    .map(|this| {
                        if !self.relays.read(cx).is_empty() {
                            this.child(self.render_list(window, cx))
                        } else {
                            this.child(self.render_empty(window, cx))
                        }
                    }),
            )
    }
}
