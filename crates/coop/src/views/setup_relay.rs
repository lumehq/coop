use std::time::Duration;

use anyhow::{anyhow, Error};
use global::constants::NIP17_RELAYS;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, uniform_list, App, AppContext, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    TextAlign, UniformList, Window,
};
use i18n::{shared_t, t};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::Registry;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::{h_flex, v_flex, ContextModal, IconName, Sizable, StyledExt};

pub fn init(kind: Kind, window: &mut Window, cx: &mut App) -> Entity<SetupRelay> {
    cx.new(|cx| SetupRelay::new(kind, window, cx))
}

pub fn setup_nip17_relay<T>(label: T) -> impl IntoElement
where
    T: Into<SharedString>,
{
    div().child(
        Button::new("setup-relays")
            .icon(IconName::Info)
            .label(label)
            .warning()
            .xsmall()
            .rounded(ButtonRounded::Full)
            .on_click(move |_, window, cx| {
                let view = cx.new(|cx| SetupRelay::new(Kind::InboxRelays, window, cx));
                let weak_view = view.downgrade();

                window.open_modal(cx, move |modal, _window, _cx| {
                    let weak_view = weak_view.clone();

                    modal
                        .confirm()
                        .title(shared_t!("relays.modal_title"))
                        .child(view.clone())
                        .button_props(ModalButtonProps::default().ok_text(t!("common.update")))
                        .on_ok(move |_, window, cx| {
                            weak_view
                                .update(cx, |this, cx| {
                                    this.set_relays(window, cx);
                                })
                                .ok();
                            // true to close the modal
                            false
                        })
                })
            }),
    )
}

pub struct SetupRelay {
    input: Entity<InputState>,
    relays: Vec<RelayUrl>,
    error: Option<SharedString>,
    _subscriptions: SmallVec<[Subscription; 1]>,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl SetupRelay {
    pub fn new(kind: Kind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let identity = Registry::read_global(cx).identity(cx).public_key();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("wss://example.com"));

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        let load_relay = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new().kind(kind).author(identity).limit(1);

            if let Some(event) = client.database().query(filter).await?.first() {
                let relays = event
                    .tags
                    .iter()
                    .filter_map(|tag| tag.as_standardized())
                    .filter_map(|tag| {
                        if let TagStandard::RelayMetadata { relay_url, .. } = tag {
                            Some(relay_url.to_owned())
                        } else if let TagStandard::Relay(url) = tag {
                            Some(url.to_owned())
                        } else {
                            None
                        }
                    })
                    .collect_vec();

                Ok(relays)
            } else {
                Err(anyhow!("Not found."))
            }
        });

        tasks.push(
            // Load user's relays in the local database
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(relays) = load_relay.await {
                    this.update(cx, |this, cx| {
                        this.relays = relays;
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
            relays: vec![],
            error: None,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();

        if !value.starts_with("ws") {
            return;
        }

        if let Ok(url) = RelayUrl::parse(&value) {
            if !self.relays.contains(&url) {
                self.relays.push(url);
            }

            self.input.update(cx, |this, cx| {
                this.set_value("", window, cx);
            });

            cx.notify();
        }
    }

    fn remove(&mut self, ix: usize, _window: &mut Window, cx: &mut Context<Self>) {
        self.relays.remove(ix);
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
            cx.update(|_, cx| {
                this.update(cx, |this, cx| {
                    this.error = None;
                    cx.notify();
                })
                .ok();
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
            let tags: Vec<Tag> = relays
                .iter()
                .map(|relay| Tag::relay(relay.clone()))
                .collect();

            let builder = EventBuilder::new(Kind::InboxRelays, "").tags(tags);

            // Set messaging relays
            client.send_event_builder(builder).await?;

            // Connect to messaging relays
            for relay in relays.into_iter() {
                _ = client.add_relay(&relay).await;
                _ = client.connect_relay(&relay).await;
            }

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
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_error(e.to_string(), window, cx);
                        })
                        .ok();
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
            .child(shared_t!("relays.add_some_relays"))
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
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().text_muted)
                                    .child(shared_t!("relays.recommended")),
                            )
                            .child(h_flex().gap_1().children({
                                NIP17_RELAYS.iter().map(|&relay| {
                                    div()
                                        .id(relay)
                                        .group("")
                                        .py_0p5()
                                        .px_1p5()
                                        .text_xs()
                                        .text_center()
                                        .bg(cx.theme().secondary_background)
                                        .hover(|this| this.bg(cx.theme().secondary_hover))
                                        .active(|this| this.bg(cx.theme().secondary_active))
                                        .rounded_full()
                                        .child(relay)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.input.update(cx, |this, cx| {
                                                this.set_value(relay, window, cx);
                                            });
                                            this.add(window, cx);
                                        }))
                                })
                            })),
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
