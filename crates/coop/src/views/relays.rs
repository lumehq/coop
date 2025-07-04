use anyhow::Error;
use global::constants::NEW_MESSAGE_SUB_ID;
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, uniform_list, App, AppContext, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, TextAlign,
    UniformList, Window,
};
use nostr_sdk::prelude::*;
use rust_i18n::t;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::{ContextModal, Disableable, IconName, Sizable};

const MIN_HEIGHT: f32 = 200.0;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Relays> {
    Relays::new(window, cx)
}

pub struct Relays {
    relays: Entity<Vec<RelayUrl>>,
    input: Entity<InputState>,
    focus_handle: FocusHandle,
    is_loading: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Relays {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(SharedString::new(t!("relays.placeholder")))
        });
        let relays = cx.new(|cx| {
            let task: Task<Result<Vec<RelayUrl>, Error>> = cx.background_spawn(async move {
                let client = shared_state().client();
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
                is_loading: false,
                focus_handle: cx.focus_handle(),
            }
        })
    }

    pub fn update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_loading(true, cx);

        let relays = self.relays.read(cx).clone();
        let task: Task<Result<EventId, Error>> = cx.background_spawn(async move {
            let client = shared_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // If user didn't have any NIP-65 relays, add default ones
            if client.database().relay_list(public_key).await?.is_empty() {
                let builder = EventBuilder::relay_list(vec![
                    (RelayUrl::parse("wss://relay.damus.io/").unwrap(), None),
                    (RelayUrl::parse("wss://relay.primal.net/").unwrap(), None),
                ]);

                if let Err(e) = client.send_event_builder(builder).await {
                    log::error!("Failed to send relay list event: {e}");
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
                log::error!("Failed to subscribe to new messages: {e}");
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

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
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

    fn render_list(
        &mut self,
        relays: Vec<RelayUrl>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> UniformList {
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
        .min_h(px(MIN_HEIGHT))
    }

    fn render_empty(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h_20()
            .mb_2()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_align(TextAlign::Center)
            .child(SharedString::new(t!("relays.add_some_relays")))
    }
}

impl Render for Relays {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .px_3()
            .pb_3()
            .flex()
            .flex_col()
            .justify_between()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_muted)
                            .child(SharedString::new(t!("relays.description"))),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w_full()
                                    .gap_2()
                                    .child(TextInput::new(&self.input).small())
                                    .child(
                                        Button::new("add_relay_btn")
                                            .icon(IconName::Plus)
                                            .label(t!("relays.add"))
                                            .small()
                                            .ghost()
                                            .rounded_md()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.add(window, cx)
                                            })),
                                    ),
                            )
                            .map(|this| {
                                let relays = self.relays.read(cx).clone();

                                if !relays.is_empty() {
                                    this.child(self.render_list(relays, window, cx))
                                } else {
                                    this.child(self.render_empty(window, cx))
                                }
                            }),
                    ),
            )
            .child(
                Button::new("submti")
                    .label(t!("relays.update"))
                    .primary()
                    .w_full()
                    .loading(self.is_loading)
                    .disabled(self.is_loading)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.update(window, cx);
                    })),
            )
    }
}
