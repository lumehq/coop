use std::{
    collections::{BTreeSet, HashSet},
    time::Duration,
};

use anyhow::Error;
use chats::ChatRegistry;
use common::profile::SharedProfile;
use global::get_client;
use gpui::{
    div, img, impl_internal_actions, prelude::FluentBuilder, px, red, relative, uniform_list, App,
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, TextAlign,
    Window,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};
use smol::Timer;
use theme::ActiveTheme;
use ui::{
    button::{Button, ButtonVariants},
    dock_area::dock::DockPlacement,
    input::{InputEvent, TextInput},
    ContextModal, Disableable, Icon, IconName, Sizable, Size, StyledExt,
};

use crate::chatspace::{AddPanel, PanelKind};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Compose> {
    cx.new(|cx| Compose::new(window, cx))
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_internal_actions!(contacts, [SelectContact]);

pub struct Compose {
    title_input: Entity<TextInput>,
    user_input: Entity<TextInput>,
    contacts: Entity<Vec<Profile>>,
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
            TextInput::new(window, cx)
                .appearance(false)
                .placeholder("Family... . (Optional)")
                .text_size(Size::Small)
        });

        let user_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(ui::Size::Small)
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

        cx.spawn(async move |this, cx| {
            let task: Task<Result<BTreeSet<Profile>, Error>> = cx.background_spawn(async move {
                let client = get_client();
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let profiles = client.database().contacts(public_key).await?;

                Ok(profiles)
            });

            if let Ok(contacts) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.contacts.update(cx, |this, cx| {
                            this.extend(contacts);
                            cx.notify();
                        });
                    })
                    .ok()
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
            subscriptions,
            is_loading: false,
            is_submitting: false,
            focus_handle: cx.focus_handle(),
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
            // this event existed just use for convert to Coop's Room later.
            let event = EventBuilder::private_msg_rumor(*pubkeys.last().unwrap(), "")
                .tags(tags)
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
                    this.set_error(Some(e.to_string().into()), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = get_client();
        let content = self.user_input.read(cx).text().to_string();

        // Show loading spinner
        self.set_loading(true, cx);

        let task: Task<Result<Profile, anyhow::Error>> = if content.contains("@") {
            cx.background_spawn(async move {
                let profile = nip05::profile(&content, None).await?;
                let public_key = profile.public_key;

                let metadata = client
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?
                    .unwrap_or_default();

                Ok(Profile::new(public_key, metadata))
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

                Ok(Profile::new(public_key, metadata))
            })
        };

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(profile) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            let public_key = profile.public_key();

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
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|_, cx| {
                        this.update(cx, |this, cx| {
                            this.set_loading(false, cx);
                            this.set_error(Some(e.to_string().into()), cx);
                        })
                        .ok();
                    })
                    .ok();
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
        const DESCRIPTION: &str =
            "Start a conversation with someone using their npub or NIP-05 (like foo@bar.com).";

        let label: SharedString = if self.selected.read(cx).len() > 1 {
            "Create Group DM".into()
        } else {
            "Create DM".into()
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_select))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_3()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(DESCRIPTION),
            )
            .when_some(self.error_message.read(cx).as_ref(), |this, msg| {
                this.child(div().px_3().text_xs().text_color(red()).child(msg.clone()))
            })
            .child(
                div().px_3().flex().flex_col().child(
                    div()
                        .h_10()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().pb_0p5().text_sm().font_semibold().child("Subject:"))
                        .child(self.title_input.clone()),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .mt_1()
                    .child(
                        div()
                            .px_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().font_semibold().child("To:"))
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
                                            .text_color(cx.theme().text_muted)
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
                                            let is_select = selected.contains(&item.public_key());

                                            items.push(
                                                div()
                                                    .id(ix)
                                                    .w_full()
                                                    .h_10()
                                                    .px_3()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_3()
                                                            .text_sm()
                                                            .child(
                                                                img(item.shared_avatar())
                                                                    .size_7()
                                                                    .flex_shrink_0(),
                                                            )
                                                            .child(item.shared_name()),
                                                    )
                                                    .when(is_select, |this| {
                                                        this.child(
                                                            Icon::new(IconName::CheckCircleFill)
                                                                .small()
                                                                .text_color(cx.theme().icon_accent),
                                                        )
                                                    })
                                                    .hover(|this| {
                                                        this.bg(cx
                                                            .theme()
                                                            .elevated_surface_background)
                                                    })
                                                    .on_click(move |_, window, cx| {
                                                        window.dispatch_action(
                                                            Box::new(SelectContact(
                                                                item.public_key(),
                                                            )),
                                                            cx,
                                                        );
                                                    }),
                                            );
                                        }

                                        items
                                    },
                                )
                                .pb_4()
                                .min_h(px(280.)),
                            )
                        }
                    }),
            )
            .child(
                div().p_3().child(
                    Button::new("create_dm_btn")
                        .label(label)
                        .primary()
                        .w_full()
                        .loading(self.is_submitting)
                        .disabled(self.is_submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.compose(window, cx))),
                ),
            )
    }
}
