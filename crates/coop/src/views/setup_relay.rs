use std::collections::HashSet;
use std::time::Duration;

use anyhow::{anyhow, Error};
use app_state::{app_state, nostr_client};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, uniform_list, App, AppContext, AsyncWindowContext, Context, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Task, TextAlign, UniformList, Window,
};
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::{h_flex, v_flex, ContextModal, IconName, Sizable};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<SetupRelay> {
    cx.new(|cx| SetupRelay::new(window, cx))
}

#[derive(Debug)]
pub struct SetupRelay {
    input: Entity<InputState>,
    error: Option<SharedString>,

    // All relays
    relays: HashSet<RelayUrl>,

    // Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    // Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl SetupRelay {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("wss://example.com"));

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        tasks.push(
            // Load user's relays in the local database
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(relays) = Self::load(cx).await {
                    this.update(cx, |this, cx| {
                        this.relays.extend(relays);
                        cx.notify();
                    })
                    .ok();
                }
            }),
        );

        subscriptions.push(
            // Subscribe to user's input events
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, _, event, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.add(window, cx);
                    }
                },
            ),
        );

        Self {
            input,
            relays: HashSet::new(),
            error: None,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    fn load(cx: &AsyncWindowContext) -> Task<Result<Vec<RelayUrl>, Error>> {
        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                let urls = nip17::extract_owned_relay_list(event).collect();
                Ok(urls)
            } else {
                Err(anyhow!("Not found."))
            }
        })
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();

        if !value.starts_with("ws") {
            self.set_error("Relay URl is invalid", window, cx);
            return;
        }

        if let Ok(url) = RelayUrl::parse(&value) {
            if !self.relays.insert(url) {
                self.input.update(cx, |this, cx| {
                    this.set_value("", window, cx);
                });
                cx.notify();
            }
        } else {
            self.set_error("Relay URl is invalid", window, cx);
        }
    }

    fn remove(&mut self, url: &RelayUrl, cx: &mut Context<Self>) {
        self.relays.remove(url);
        cx.notify();
    }

    fn set_error<E>(&mut self, error: E, window: &mut Window, cx: &mut Context<Self>)
    where
        E: Into<SharedString>,
    {
        self.error = Some(error.into());
        cx.notify();

        // Clear the error message after a delay
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;

            this.update(cx, |this, cx| {
                this.error = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn set_relays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.relays.is_empty() {
            self.set_error(t!("relays.empty"), window, cx);
            return;
        };

        let relays = self.relays.clone();

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let app_state = app_state();
            let gossip = app_state.gossip.read().await;

            let tags: Vec<Tag> = relays
                .iter()
                .map(|relay| Tag::relay(relay.clone()))
                .collect();

            let event = EventBuilder::new(Kind::InboxRelays, "")
                .tags(tags)
                .sign(&signer)
                .await?;

            // Set messaging relays
            gossip.send_event_to_write_relays(&event).await?;

            // Connect to messaging relays
            for relay in relays.iter() {
                client.add_relay(relay).await.ok();
                client.connect_relay(relay).await.ok();
            }

            // Fetch gift wrap events
            let sub_id = app_state.inner.gift_wrap_sub_id.clone();
            let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

            if client
                .subscribe_with_id_to(relays.clone(), sub_id, filter, None)
                .await
                .is_ok()
            {
                log::info!("Subscribed to messages in: {relays:?}");
            };

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(_) => {
                    cx.update(|window, cx| {
                        window.close_modal(cx);
                    })
                    .ok();
                }
                Err(e) => {
                    this.update_in(cx, |this, window, cx| {
                        this.set_error(e.to_string(), window, cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn render_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> UniformList {
        let relays = self.relays.clone();
        let total = relays.len();

        uniform_list(
            "relays",
            total,
            cx.processor(move |_v, range, _window, cx| {
                let mut items = Vec::new();

                for ix in range {
                    if let Some(url) = relays.iter().nth(ix) {
                        items.push(
                            div()
                                .id(SharedString::from(url.to_string()))
                                .group("")
                                .w_full()
                                .h_9()
                                .py_0p5()
                                .child(
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
                                        .child(SharedString::from(url.to_string()))
                                        .child(
                                            Button::new("remove_{ix}")
                                                .icon(IconName::Close)
                                                .xsmall()
                                                .ghost()
                                                .invisible()
                                                .group_hover("", |this| this.visible())
                                                .on_click({
                                                    let url = url.to_owned();
                                                    cx.listener(move |this, _ev, _window, cx| {
                                                        this.remove(&url, cx);
                                                    })
                                                }),
                                        ),
                                ),
                        )
                    }
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
            .child(shared_t!("relays.help_text"))
    }
}

impl Render for SetupRelay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().text_muted)
                    .child(shared_t!("relays.description")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_1()
                            .w_full()
                            .child(TextInput::new(&self.input).small())
                            .child(
                                Button::new("add")
                                    .icon(IconName::PlusFill)
                                    .label(t!("common.add"))
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.add(window, cx);
                                    })),
                            ),
                    )
                    .when_some(self.error.as_ref(), |this, error| {
                        this.child(
                            div()
                                .italic()
                                .text_xs()
                                .text_color(cx.theme().danger_foreground)
                                .child(error.clone()),
                        )
                    }),
            )
            .map(|this| {
                if !self.relays.is_empty() {
                    this.child(self.render_list(window, cx))
                } else {
                    this.child(self.render_empty(window, cx))
                }
            })
    }
}
