use std::time::Duration;

use anyhow::Error;
use async_utility::task::spawn;
use chats::ChatRegistry;
use common::profile::SharedProfile;
use global::{constants::SEARCH_RELAYS, get_client};
use gpui::{
    div, img, prelude::FluentBuilder, px, red, relative, uniform_list, App, AppContext, Context,
    Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::{
    button::{Button, ButtonVariants},
    dock_area::dock::DockPlacement,
    indicator::Indicator,
    input::{InputEvent, TextInput},
    ContextModal, Disableable, IconName, Sizable,
};

use crate::chatspace::{AddPanel, PanelKind};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Search> {
    Search::new(window, cx)
}

pub struct Search {
    input: Entity<TextInput>,
    result: Entity<Vec<Profile>>,
    error: Entity<Option<SharedString>>,
    loading: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Search {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let result = cx.new(|_| vec![]);
        let error = cx.new(|_| None);
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(ui::Size::Small)
                .placeholder("type something...")
        });

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Search, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.search(window, cx);
                    }
                },
            ));

            Self {
                input,
                result,
                error,
                subscriptions,
                loading: false,
            }
        })
    }

    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        };

        // Show loading spinner
        self.loading(true, cx);

        // Get search query
        let query = self.input.read(cx).text();

        let task: Task<Result<Vec<Profile>, Error>> = cx.background_spawn(async move {
            let client = get_client();

            let filter = Filter::new()
                .kind(Kind::Metadata)
                .search(query.to_lowercase())
                .limit(10);

            let events = client
                .fetch_events_from(SEARCH_RELAYS, filter, Duration::from_secs(3))
                .await?
                .into_iter()
                .unique_by(|event| event.pubkey)
                .collect_vec();

            let mut users = vec![];
            let (tx, rx) = smol::channel::bounded::<Profile>(events.len());

            spawn(async move {
                for event in events.into_iter() {
                    let metadata = Metadata::from_json(event.content).unwrap_or_default();

                    if let Some(target) = metadata.nip05.as_ref() {
                        if let Ok(verify) = nip05::verify(&event.pubkey, target, None).await {
                            if verify {
                                _ = tx.send(Profile::new(event.pubkey, metadata)).await;
                            }
                        }
                    }
                }
            });

            while let Ok(profile) = rx.recv().await {
                users.push(profile);
            }

            Ok(users)
        });

        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(users) => {
                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        this.loading(false, cx);
                        this.result.update(cx, |this, cx| {
                            *this = users;
                            cx.notify();
                        });
                    })
                    .ok();
                })
                .ok();
            }
            Err(error) => {
                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        this.loading(false, cx);
                        this.error.update(cx, |this, cx| {
                            *this = Some(error.to_string().into());
                            cx.notify();
                        });
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn chat(&mut self, to: Profile, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = to.public_key();

        let event: Task<Result<Event, anyhow::Error>> = cx.background_spawn(async move {
            let client = get_client();
            let signer = client.signer().await?;
            // [IMPORTANT]
            // Make sure this event is never send,
            // this event existed just use for convert to Coop's Room later.
            let event = EventBuilder::private_msg_rumor(public_key, "")
                .sign(&signer)
                .await?;

            Ok(event)
        });

        cx.spawn_in(window, async move |this, cx| match event.await {
            Ok(event) => {
                cx.update(|window, cx| {
                    ChatRegistry::global(cx).update(cx, |chats, cx| {
                        let id = chats.push(&event, window, cx);
                        window.close_modal(cx);
                        window.dispatch_action(
                            Box::new(AddPanel::new(PanelKind::Room(id), DockPlacement::Center)),
                            cx,
                        );
                    });
                })
                .ok();
            }
            Err(e) => {
                this.update(cx, |this, cx| {
                    this.error.update(cx, |this, cx| {
                        *this = Some(e.to_string().into());
                        cx.notify();
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    fn loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.loading = status;
        cx.notify();
    }
}

impl Render for Search {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .mt_3()
            .child(
                div().px_3().child(
                    div()
                        .flex()
                        .gap_1()
                        .items_center()
                        .child(self.input.clone())
                        .child(
                            Button::new("find")
                                .icon(IconName::Search)
                                .ghost()
                                .disabled(self.loading)
                                .on_click(
                                    cx.listener(move |this, _, window, cx| this.search(window, cx)),
                                ),
                        ),
                ),
            )
            .when_some(self.error.read(cx).as_ref(), |this, error| {
                this.child(
                    div()
                        .px_3()
                        .text_xs()
                        .text_color(red())
                        .child(error.clone()),
                )
            })
            .child(div().map(|this| {
                let result = self.result.read(cx).clone();

                if self.loading {
                    this.h_32()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Indicator::new().small())
                } else if result.is_empty() {
                    this.h_32()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(cx.theme().text_muted)
                        .child("No one with that query could be found.")
                } else {
                    this.child(
                        uniform_list(
                            cx.entity(),
                            "find-result",
                            result.len(),
                            move |_, range, _window, cx| {
                                let mut items = Vec::new();

                                for ix in range {
                                    let item = result.get(ix).cloned().unwrap();

                                    items.push(
                                        div()
                                            .id(ix)
                                            .group("")
                                            .w_full()
                                            .h_12()
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded(cx.theme().radius)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        img(item.shared_avatar())
                                                            .size_8()
                                                            .flex_shrink_0(),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .child(
                                                                div()
                                                                    .text_sm()
                                                                    .line_height(relative(1.2))
                                                                    .child(item.shared_name()),
                                                            )
                                                            .when_some(
                                                                item.metadata().nip05,
                                                                |this, nip05| {
                                                                    this.child(
                                                                        div()
                                                                            .text_xs()
                                                                            .text_color(
                                                                                cx.theme()
                                                                                    .text_muted,
                                                                            )
                                                                            .child(nip05),
                                                                    )
                                                                },
                                                            ),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .invisible()
                                                    .group_hover("", |this| this.visible())
                                                    .child(
                                                        Button::new(ix)
                                                            .icon(IconName::ArrowRight)
                                                            .label("Chat")
                                                            .xsmall()
                                                            .primary()
                                                            .reverse()
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    this.chat(
                                                                        item.clone(),
                                                                        window,
                                                                        cx,
                                                                    );
                                                                },
                                                            )),
                                                    ),
                                            )
                                            .hover(|this| {
                                                this.bg(cx.theme().elevated_surface_background)
                                            }),
                                    );
                                }

                                items
                            },
                        )
                        .min_h(px(150.)),
                    )
                }
            }))
    }
}
