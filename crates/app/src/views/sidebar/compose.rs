use chats::{room::Room, ChatRegistry};
use common::{profile::NostrProfile, utils::random_name};
use global::{constants::DEVICE_ANNOUNCEMENT_KIND, get_client};
use gpui::{
    div, img, impl_internal_actions, prelude::FluentBuilder, px, relative, uniform_list, App,
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, TextAlign,
    Window,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};
use smol::Timer;
use std::{collections::HashSet, time::Duration};
use ui::{
    button::{Button, ButtonRounded},
    input::{InputEvent, TextInput},
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Icon, IconName, Sizable, Size, StyledExt,
};

const DESCRIPTION: &str =
    "Start a conversation with someone using their npub or NIP-05 (like foo@bar.com).";

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_internal_actions!(contacts, [SelectContact]);

pub struct Compose {
    title_input: Entity<TextInput>,
    user_input: Entity<TextInput>,
    contacts: Entity<Vec<NostrProfile>>,
    selected: Entity<HashSet<PublicKey>>,
    focus_handle: FocusHandle,
    is_loading: bool,
    is_submitting: bool,
    error_message: Entity<Option<SharedString>>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let contacts = cx.new(|_| Vec::new());
        let selected = cx.new(|_| HashSet::new());
        let error_message = cx.new(|_| None);

        let title_input = cx.new(|cx| {
            let name = random_name(2);
            let mut input = TextInput::new(window, cx)
                .appearance(false)
                .text_size(Size::XSmall);

            input.set_placeholder("Family... . (Optional)");
            input.set_text(name, window, cx);
            input
        });

        let user_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(ui::Size::Small)
                .small()
                .placeholder("npub1...")
        });

        let mut subscriptions = smallvec![];

        // Handle Enter event for user input
        subscriptions.push(cx.subscribe_in(
            &user_input,
            window,
            move |this, _, input_event, window, cx| {
                if let InputEvent::PressEnter = input_event {
                    this.add(window, cx);
                }
            },
        ));

        let client = get_client();
        let (tx, rx) = oneshot::channel::<Vec<NostrProfile>>();

        cx.background_spawn(async move {
            let signer = client.signer().await.unwrap();
            let public_key = signer.get_public_key().await.unwrap();

            if let Ok(profiles) = client.database().contacts(public_key).await {
                let members: Vec<NostrProfile> = profiles
                    .into_iter()
                    .map(|profile| NostrProfile::new(profile.public_key(), profile.metadata()))
                    .collect();

                _ = tx.send(members);
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            if let Ok(contacts) = rx.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.contacts.update(cx, |this, cx| {
                            this.extend(contacts);
                            cx.notify();
                        });
                    })
                })
                .ok();
            }
        })
        .detach();

        Self {
            title_input,
            user_input,
            contacts,
            selected,
            error_message,
            is_loading: false,
            is_submitting: false,
            focus_handle: cx.focus_handle(),
            subscriptions,
        }
    }

    pub fn compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.read(cx).is_empty() {
            self.set_error(Some("You need to add at least 1 receiver".into()), cx);
            return;
        }

        // Show loading spinner
        self.set_submitting(true, cx);

        // Get all pubkeys
        let pubkeys: Vec<PublicKey> = self.selected.read(cx).iter().copied().collect();

        // Convert selected pubkeys into Nostr tags
        let mut tag_list: Vec<Tag> = pubkeys.iter().map(|pk| Tag::public_key(*pk)).collect();

        // Add subject if it is present
        if !self.title_input.read(cx).text().is_empty() {
            tag_list.push(Tag::custom(
                TagKind::Subject,
                vec![self.title_input.read(cx).text().to_string()],
            ));
        }

        let tags = Tags::from_list(tag_list);

        let event: Task<Result<Event, anyhow::Error>> = cx.background_spawn(async move {
            let client = get_client();
            let signer = client.signer().await?;
            // [IMPORTANT]
            // Make sure this event is never send,
            // this event existed just use for convert to Coop's Chat Room later.
            let event = EventBuilder::private_msg_rumor(*pubkeys.last().unwrap(), "")
                .tags(tags)
                .sign(&signer)
                .await?;

            Ok(event)
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(event) = event.await {
                cx.update(|window, cx| {
                    // Stop loading spinner
                    this.update(cx, |this, cx| {
                        this.set_submitting(false, cx);
                    })
                    .ok();

                    let chats = ChatRegistry::global(cx);
                    let room = Room::new(&event, cx);

                    chats.update(cx, |state, cx| {
                        match state.push_room(room, cx) {
                            Ok(_) => {
                                // TODO: automatically open newly created chat panel
                                window.close_modal(cx);
                            }
                            Err(e) => {
                                _ = this.update(cx, |this, cx| {
                                    this.set_error(Some(e.to_string().into()), cx);
                                });
                            }
                        }
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn label(&self, _window: &Window, cx: &App) -> SharedString {
        if self.selected.read(cx).len() > 1 {
            "Create Group DM".into()
        } else {
            "Create DM".into()
        }
    }

    pub fn is_submitting(&self) -> bool {
        self.is_submitting
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = get_client();
        let window_handle = window.window_handle();
        let content = self.user_input.read(cx).text().to_string();

        // Show loading spinner
        self.set_loading(true, cx);

        let task: Task<Result<NostrProfile, anyhow::Error>> = if content.contains("@") {
            cx.background_spawn(async move {
                let profile = nip05::profile(&content, None).await?;
                let public_key = profile.public_key;

                let metadata = client
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?
                    .unwrap_or_default();

                Ok(NostrProfile::new(public_key, metadata))
            })
        } else {
            let Ok(public_key) = PublicKey::parse(&content) else {
                self.set_loading(false, cx);
                self.set_error(Some("Public Key is not valid".into()), cx);
                return;
            };

            cx.background_spawn(async move {
                let metadata = client
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?
                    .unwrap_or_default();

                Ok(NostrProfile::new(public_key, metadata))
            })
        };

        cx.spawn(async move |this, cx| {
            match task.await {
                Ok(profile) => {
                    let public_key = profile.public_key;

                    _ = cx
                        .background_spawn(async move {
                            let opts = SubscribeAutoCloseOptions::default()
                                .exit_policy(ReqExitPolicy::ExitOnEOSE);

                            // Create a device announcement filter
                            let device = Filter::new()
                                .kind(Kind::Custom(DEVICE_ANNOUNCEMENT_KIND))
                                .author(public_key)
                                .limit(1);

                            // Only subscribe to the latest device announcement
                            client.subscribe(device, Some(opts)).await
                        })
                        .await;

                    _ = cx.update_window(window_handle, |_, window, cx| {
                        _ = this.update(cx, |this, cx| {
                            let public_key = profile.public_key;

                            this.contacts.update(cx, |this, cx| {
                                this.insert(0, profile);
                                cx.notify();
                            });

                            this.selected.update(cx, |this, cx| {
                                this.insert(public_key);
                                cx.notify();
                            });

                            // Stop loading indicator
                            this.set_loading(false, cx);

                            // Clear input
                            this.user_input.update(cx, |this, cx| {
                                this.set_text("", window, cx);
                                cx.notify();
                            });
                        });
                    });
                }
                Err(e) => {
                    _ = cx.update_window(window_handle, |_, _, cx| {
                        _ = this.update(cx, |this, cx| {
                            this.set_loading(false, cx);
                            this.set_error(Some(e.to_string().into()), cx);
                        });
                    });
                }
            }
        })
        .detach();
    }

    fn set_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.error_message.update(cx, |this, cx| {
            *this = error;
            cx.notify();
        });

        // Dismiss error after 2 seconds
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_secs(2)).await;

            cx.update(|cx| {
                this.update(cx, |this, cx| {
                    this.set_error(None, cx);
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
        cx.notify();
    }

    fn set_submitting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_submitting = status;
        cx.notify();
    }

    fn on_action_select(
        &mut self,
        action: &SelectContact,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected.update(cx, |this, cx| {
            if this.contains(&action.0) {
                this.remove(&action.0);
            } else {
                this.insert(action.0);
            };
            cx.notify();
        });
    }
}

impl Render for Compose {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_select))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .child(DESCRIPTION),
            )
            .when_some(self.error_message.read(cx).as_ref(), |this, msg| {
                this.child(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(msg.clone()),
                )
            })
            .child(
                div().flex().flex_col().child(
                    div()
                        .h_10()
                        .px_2()
                        .border_b_1()
                        .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_xs().font_semibold().child("Title:"))
                        .child(self.title_input.clone()),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().px_2().text_xs().font_semibold().child("To:"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .child(
                                Button::new("add_user_to_compose_btn")
                                    .icon(IconName::Plus)
                                    .small()
                                    .rounded(ButtonRounded::Size(px(9999.)))
                                    .loading(self.is_loading)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add(window, cx);
                                    })),
                            )
                            .child(self.user_input.clone()),
                    )
                    .map(|this| {
                        let contacts = self.contacts.read(cx).clone();
                        let view = cx.entity();

                        if contacts.is_empty() {
                            this.child(
                                div()
                                    .w_full()
                                    .h_24()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .text_align(TextAlign::Center)
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .line_height(relative(1.2))
                                            .child("No contacts"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(
                                                cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
                                            )
                                            .child("Your recently contacts will appear here."),
                                    ),
                            )
                        } else {
                            this.child(
                                uniform_list(
                                    view,
                                    "contacts",
                                    contacts.len(),
                                    move |this, range, _window, cx| {
                                        let selected = this.selected.read(cx);
                                        let mut items = Vec::new();

                                        for ix in range {
                                            let item = contacts.get(ix).unwrap().clone();
                                            let is_select = selected.contains(&item.public_key);

                                            items.push(
                                                div()
                                                    .id(ix)
                                                    .w_full()
                                                    .h_9()
                                                    .px_2()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .text_xs()
                                                            .child(
                                                                div().flex_shrink_0().child(
                                                                    img(item.avatar).size_6(),
                                                                ),
                                                            )
                                                            .child(item.name),
                                                    )
                                                    .when(is_select, |this| {
                                                        this.child(
                                                            Icon::new(IconName::CircleCheck)
                                                                .size_3()
                                                                .text_color(cx.theme().base.step(
                                                                    cx,
                                                                    ColorScaleStep::TWELVE,
                                                                )),
                                                        )
                                                    })
                                                    .hover(|this| {
                                                        this.bg(cx
                                                            .theme()
                                                            .base
                                                            .step(cx, ColorScaleStep::THREE))
                                                    })
                                                    .on_click(move |_, window, cx| {
                                                        window.dispatch_action(
                                                            Box::new(SelectContact(
                                                                item.public_key,
                                                            )),
                                                            cx,
                                                        );
                                                    }),
                                            );
                                        }

                                        items
                                    },
                                )
                                .min_h(px(250.)),
                            )
                        }
                    }),
            )
    }
}
