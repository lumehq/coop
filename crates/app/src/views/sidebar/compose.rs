use chat_state::registry::ChatRegistry;
use common::{
    constants::FAKE_SIG,
    profile::NostrProfile,
    utils::{random_name, signer_public_key},
};
use gpui::{
    div, img, impl_internal_actions, prelude::FluentBuilder, px, relative, uniform_list, App,
    AppContext, BorrowAppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, TextAlign, Window,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use state::get_client;
use std::{collections::HashSet, str::FromStr, time::Duration};
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonRounded},
    input::{InputEvent, TextInput},
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Icon, IconName, Sizable, Size, StyledExt,
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_internal_actions!(contacts, [SelectContact]);

pub struct Compose {
    title_input: Entity<TextInput>,
    message_input: Entity<TextInput>,
    user_input: Entity<TextInput>,
    contacts: Entity<Vec<NostrProfile>>,
    selected: Entity<HashSet<PublicKey>>,
    focus_handle: FocusHandle,
    is_loading: bool,
    is_submitting: bool,
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let contacts = cx.new(|_| Vec::new());
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
                .placeholder("Hello...")
        });

        // Handle Enter event for message input
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
            let (tx, rx) = oneshot::channel::<Vec<NostrProfile>>();

            cx.background_executor()
                .spawn(async move {
                    let client = get_client();
                    if let Ok(public_key) = signer_public_key(client).await {
                        if let Ok(profiles) = client.database().contacts(public_key).await {
                            let members: Vec<NostrProfile> = profiles
                                .into_iter()
                                .map(|profile| {
                                    NostrProfile::new(profile.public_key(), profile.metadata())
                                })
                                .collect();

                            _ = tx.send(members);
                        }
                    }
                })
                .detach();

            if let Ok(contacts) = rx.await {
                if let Some(view) = this.upgrade() {
                    _ = cx.update_entity(&view, |this, cx| {
                        this.contacts.update(cx, |this, cx| {
                            this.extend(contacts);
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
            is_submitting: false,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected.read(cx).to_owned();
        let message = self.message_input.read(cx).text();

        if selected.is_empty() {
            window.push_notification("You need to add at least 1 receiver", cx);
            return;
        }

        if message.is_empty() {
            window.push_notification("Message is required", cx);
            return;
        }

        // Show loading spinner
        self.set_submitting(true, cx);

        // Get message from user's input
        let content = message.to_string();

        // Get room title from user's input
        let title = Tag::custom(
            TagKind::Subject,
            vec![self.title_input.read(cx).text().to_string()],
        );

        // Get all pubkeys
        let mut pubkeys: Vec<PublicKey> = selected.iter().copied().collect();

        // Convert selected pubkeys into Nostr tags
        let mut tag_list: Vec<Tag> = selected.iter().map(|pk| Tag::public_key(*pk)).collect();
        tag_list.push(title);

        let tags = Tags::new(tag_list);
        let window_handle = window.window_handle();

        cx.spawn(|this, mut cx| async move {
            let (tx, rx) = oneshot::channel::<Event>();

            cx.background_spawn(async move {
                let client = get_client();
                let public_key = signer_public_key(client).await.unwrap();
                let mut event: Option<Event> = None;

                pubkeys.push(public_key);

                for pubkey in pubkeys.iter() {
                    if let Ok(output) = client
                        .send_private_msg(*pubkey, &content, tags.clone())
                        .await
                    {
                        if pubkey == &public_key && event.is_none() {
                            if let Ok(Some(ev)) = client.database().event_by_id(&output.val).await {
                                if let Ok(UnwrappedGift { mut rumor, .. }) =
                                    client.unwrap_gift_wrap(&ev).await
                                {
                                    // Compute event id if not exist
                                    rumor.ensure_id();

                                    if let Some(id) = rumor.id {
                                        let ev = Event::new(
                                            id,
                                            rumor.pubkey,
                                            rumor.created_at,
                                            rumor.kind,
                                            rumor.tags,
                                            rumor.content,
                                            Signature::from_str(FAKE_SIG).unwrap(),
                                        );

                                        event = Some(ev);
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(event) = event {
                    _ = tx.send(event);
                }
            })
            .detach();

            if let Ok(event) = rx.await {
                _ = cx.update_window(window_handle, |_, window, cx| {
                    cx.update_global::<ChatRegistry, _>(|this, cx| {
                        this.new_room_message(event, window, cx);
                    });

                    // Stop loading spinner
                    _ = this.update(cx, |this, cx| {
                        this.set_submitting(false, cx);
                    });

                    // Close modal
                    window.close_modal(cx);
                });
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
        let window_handle = window.window_handle();
        let content = self.user_input.read(cx).text().to_string();

        // Show loading spinner
        self.set_loading(true, cx);

        if let Ok(public_key) = PublicKey::parse(&content) {
            if self
                .contacts
                .read(cx)
                .iter()
                .any(|c| c.public_key() == public_key)
            {
                self.set_loading(false, cx);
                return;
            };

            cx.spawn(|this, mut cx| async move {
                let (tx, rx) = oneshot::channel::<Metadata>();

                cx.background_spawn(async move {
                    let client = get_client();
                    let metadata = (client
                        .fetch_metadata(public_key, Duration::from_secs(3))
                        .await)
                        .unwrap_or_default();

                    _ = tx.send(metadata);
                })
                .detach();

                if let Ok(metadata) = rx.await {
                    _ = cx.update_window(window_handle, |_, window, cx| {
                        _ = this.update(cx, |this, cx| {
                            this.contacts.update(cx, |this, cx| {
                                this.insert(0, NostrProfile::new(public_key, metadata));
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
            })
            .detach();
        } else {
            self.set_loading(false, cx);
            window.push_notification("Public Key is not valid", cx);
        }
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
                                Button::new("add_user_to_compose_btn")
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
                                .min_h(px(250.)),
                            )
                        }
                    }),
            )
    }
}
