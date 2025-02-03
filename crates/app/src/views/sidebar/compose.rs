use app_state::registry::AppRegistry;
use chat_state::room::Room;
use common::{
    profile::NostrProfile,
    utils::{random_name, room_hash},
};
use gpui::{
    div, img, impl_internal_actions, px, uniform_list, App, AppContext, Context, Entity,
    FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use state::get_client;
use std::{collections::HashSet, time::Duration};
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonRounded},
    indicator::Indicator,
    input::{InputEvent, TextInput},
    prelude::FluentBuilder,
    theme::{scale::ColorScaleStep, ActiveTheme},
    Icon, IconName, Sizable, Size, StyledExt,
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_internal_actions!(contacts, [SelectContact]);

pub struct Compose {
    title_input: Entity<TextInput>,
    message_input: Entity<TextInput>,
    user_input: Entity<TextInput>,
    contacts: Entity<Option<Vec<NostrProfile>>>,
    selected: Entity<HashSet<PublicKey>>,
    focus_handle: FocusHandle,
    is_loading: bool,
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let contacts = cx.new(|_| None);
        let selected = cx.new(|_| HashSet::new());

        let user_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(ui::Size::Small)
                .small()
                .placeholder("npub1...")
        });

        let title_input = cx.new(|cx| {
            let name = random_name(2);
            let mut input = TextInput::new(window, cx)
                .appearance(false)
                .text_size(Size::XSmall);

            input.set_placeholder("Family... . (Optional)");
            input.set_text(name, window, cx);
            input
        });

        let message_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .appearance(false)
                .text_size(Size::XSmall)
                .placeholder("Hello... (Optional)")
        });

        cx.subscribe_in(
            &user_input,
            window,
            move |this, _, input_event, window, cx| {
                if let InputEvent::PressEnter = input_event {
                    this.add(window, cx);
                }
            },
        )
        .detach();

        cx.spawn(|this, mut cx| async move {
            let client = get_client();
            let (tx, rx) = oneshot::channel::<Vec<NostrProfile>>();

            cx.background_executor()
                .spawn(async move {
                    let signer = client.signer().await.unwrap();
                    let public_key = signer.get_public_key().await.unwrap();

                    if let Ok(profiles) = client.database().contacts(public_key).await {
                        let members: Vec<NostrProfile> = profiles
                            .into_iter()
                            .map(|profile| {
                                NostrProfile::new(profile.public_key(), profile.metadata())
                            })
                            .collect();

                        _ = tx.send(members);
                    }
                })
                .detach();

            if let Ok(contacts) = rx.await {
                if let Some(view) = this.upgrade() {
                    _ = cx.update_entity(&view, |this, cx| {
                        this.contacts.update(cx, |this, cx| {
                            *this = Some(contacts);
                            cx.notify();
                        });
                        cx.notify();
                    });
                }
            }
        })
        .detach();

        Self {
            title_input,
            message_input,
            user_input,
            contacts,
            selected,
            is_loading: false,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn room(&self, _window: &Window, cx: &App) -> Option<Room> {
        if let Some(current_user) = cx.global::<AppRegistry>().user() {
            // Convert selected pubkeys into nostr tags
            let tags: Vec<Tag> = self
                .selected
                .read(cx)
                .iter()
                .map(|pk| Tag::public_key(*pk))
                .collect();
            let tags = Tags::new(tags);

            // Convert selected pubkeys into members
            let members: Vec<NostrProfile> = self
                .selected
                .read(cx)
                .clone()
                .into_iter()
                .map(|pk| NostrProfile::new(pk, Metadata::new()))
                .collect();

            // Get room's id
            let id = room_hash(&tags);

            // Get room's owner (current user)
            let owner = NostrProfile::new(current_user.public_key(), Metadata::new());

            // Get room's title
            let title = self.title_input.read(cx).text().to_string().into();

            Some(Room::new(id, owner, members, Some(title), Timestamp::now()))
        } else {
            None
        }
    }

    pub fn label(&self, _window: &Window, cx: &App) -> SharedString {
        if self.selected.read(cx).len() > 1 {
            "Create Group DM".into()
        } else {
            "Create DM".into()
        }
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let window_handle = window.window_handle();
        let content = self.user_input.read(cx).text().to_string();
        let input = self.user_input.downgrade();

        // Show loading spinner
        self.set_loading(true, cx);

        if let Ok(public_key) = PublicKey::parse(&content) {
            cx.spawn(|this, mut async_cx| async move {
                let query: anyhow::Result<Metadata, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let client = get_client();
                        let metadata = client
                            .fetch_metadata(public_key, Duration::from_secs(3))
                            .await?;

                        Ok(metadata)
                    })
                    .await;

                if let Ok(metadata) = query {
                    if let Some(view) = this.upgrade() {
                        _ = async_cx.update_entity(&view, |this, cx| {
                            this.contacts.update(cx, |this, cx| {
                                if let Some(members) = this {
                                    members.insert(0, NostrProfile::new(public_key, metadata));
                                }
                                cx.notify();
                            });

                            this.selected.update(cx, |this, cx| {
                                this.insert(public_key);
                                cx.notify();
                            });

                            this.set_loading(false, cx);
                        });
                    }

                    if let Some(input) = input.upgrade() {
                        _ = async_cx.update_window(window_handle, |_, window, cx| {
                            cx.update_entity(&input, |this, cx| {
                                this.set_text("", window, cx);
                            })
                        });
                    }
                }
            })
            .detach();
        } else {
            // Handle error
        }
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_loading = status;
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
        let msg =
            "Start a conversation with someone using their npub or NIP-05 (like foo@bar.com).";

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
                    .child(msg),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
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
                    )
                    .child(
                        div()
                            .h_10()
                            .px_2()
                            .border_b_1()
                            .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().text_xs().font_semibold().child("Message:"))
                            .child(self.message_input.clone()),
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
                                Button::new("add")
                                    .icon(IconName::Plus)
                                    .small()
                                    .rounded(ButtonRounded::Size(px(9999.)))
                                    .loading(self.is_loading)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.add(window, cx)),
                                    ),
                            )
                            .child(self.user_input.clone()),
                    )
                    .map(|this| {
                        if let Some(contacts) = self.contacts.read(cx).clone() {
                            this.child(
                                uniform_list(
                                    cx.entity().clone(),
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
                                                                    img(item.avatar()).size_6(),
                                                                ),
                                                            )
                                                            .child(item.name()),
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
                                .h(px(300.)),
                            )
                        } else {
                            this.flex()
                                .items_center()
                                .justify_center()
                                .h_16()
                                .child(Indicator::new().small())
                        }
                    }),
            )
    }
}
