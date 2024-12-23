use coop_ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    theme::ActiveTheme,
    v_flex, Icon, IconName, StyledExt,
};
use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::{collections::HashMap, sync::Arc};

use crate::{
    get_client,
    states::chat::{ChatRegistry, Room},
    utils::{ago, show_npub},
};

#[derive(Clone, Debug, IntoElement)]
pub struct MessageItem {
    author: PublicKey,
    metadata: Option<Metadata>,
    content: SharedString,
    created_at: Timestamp,
}

impl MessageItem {
    pub fn new(
        author: PublicKey,
        metadata: Option<Metadata>,
        content: String,
        created_at: Timestamp,
    ) -> Self {
        MessageItem {
            author,
            metadata,
            created_at,
            content: content.into(),
        }
    }
}

impl RenderOnce for MessageItem {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let ago = ago(self.created_at.as_u64());
        let fallback_name = show_npub(self.author, 16);

        div()
            .flex()
            .gap_3()
            .w_full()
            .p_2()
            .child(div().flex_shrink_0().map(|this| {
                if let Some(metadata) = self.metadata.clone() {
                    if let Some(picture) = metadata.picture {
                        this.child(
                            img(picture)
                                .size_8()
                                .rounded_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    } else {
                        this.child(img("brand/avatar.png").size_8().rounded_full())
                    }
                } else {
                    this.child(img("brand/avatar.png").size_8().rounded_full())
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_initial()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .text_xs()
                            .child(div().font_semibold().map(|this| {
                                if let Some(metadata) = self.metadata {
                                    if let Some(display_name) = metadata.display_name {
                                        this.child(display_name)
                                    } else {
                                        this.child(fallback_name)
                                    }
                                } else {
                                    this.child(fallback_name)
                                }
                            }))
                            .child(div().child(ago).text_color(cx.theme().muted_foreground)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(self.content),
                    ),
            )
    }
}

#[derive(Clone)]
pub struct Messages {
    count: usize,
    items: Vec<MessageItem>,
}

pub struct ChatRoom {
    owner: PublicKey,
    members: Arc<[PublicKey]>,
    // Form
    input: View<TextInput>,
    // Messages
    list: ListState,
    messages: Model<Messages>,
}

impl ChatRoom {
    pub fn new(room: &Arc<Room>, cx: &mut ViewContext<'_, Self>) -> Self {
        let members: Arc<[PublicKey]> = room.members.clone().into();
        let owner = room.owner;

        // Form
        let input = cx.new_view(|cx| {
            TextInput::new(cx)
                .appearance(false)
                .text_size(coop_ui::Size::Small)
                .placeholder("Message...")
                .cleanable()
        });

        // Send message when user presses enter on form.
        cx.subscribe(&input, move |this, _, input_event, cx| {
            if let InputEvent::PressEnter = input_event {
                this.send_message(cx);
            }
        })
        .detach();

        let messages = cx.new_model(|_| Messages {
            count: 0,
            items: vec![],
        });

        cx.observe(&messages, |this, model, cx| {
            let items = model.read(cx).items.clone();

            this.list = ListState::new(
                items.len(),
                ListAlignment::Bottom,
                Pixels(256.),
                move |idx, _cx| {
                    let item = items.get(idx).unwrap().clone();
                    div().child(item).into_any_element()
                },
            );

            cx.notify();
        })
        .detach();

        let list = ListState::new(0, ListAlignment::Bottom, Pixels(256.), move |_, _| {
            div().into_any_element()
        });

        Self {
            owner,
            members,
            input,
            list,
            messages,
        }
    }

    pub fn load(&self, cx: &mut ViewContext<Self>) {
        let async_messages = self.messages.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();
                let members = self.members.to_vec();

                async move {
                    let signer = client.signer().await.unwrap();
                    let public_key = signer.get_public_key().await.unwrap();

                    let recv = Filter::new()
                        .kind(Kind::PrivateDirectMessage)
                        .authors(members.clone())
                        .pubkey(public_key);

                    let send = Filter::new()
                        .kind(Kind::PrivateDirectMessage)
                        .author(public_key)
                        .pubkeys(members);

                    let events = async_cx
                        .background_executor()
                        .spawn(async move { client.database().query(vec![recv, send]).await })
                        .await;

                    if let Ok(events) = events {
                        let public_keys: Vec<PublicKey> = events
                            .iter()
                            .unique_by(|ev| ev.pubkey)
                            .map(|ev| ev.pubkey)
                            .collect();

                        let mut profiles = async_cx
                            .background_executor()
                            .spawn(async move {
                                let mut data: HashMap<PublicKey, Option<Metadata>> = HashMap::new();

                                for public_key in public_keys.into_iter() {
                                    if let Ok(metadata) =
                                        client.database().metadata(public_key).await
                                    {
                                        data.insert(public_key, metadata);
                                    }
                                }

                                data
                            })
                            .await;

                        let items: Vec<MessageItem> = events
                            .into_iter()
                            .sorted_by_key(|ev| ev.created_at)
                            .map(|ev| {
                                // Get user's metadata
                                let metadata = profiles.get_mut(&ev.pubkey).and_then(Option::take);
                                // Return message item
                                MessageItem::new(ev.pubkey, metadata, ev.content, ev.created_at)
                            })
                            .collect();

                        let total = items.len();

                        _ = async_cx.update_model(&async_messages, |a, b| {
                            a.items = items;
                            a.count = total;
                            b.notify();
                        });
                    }
                }
            })
            .detach();
    }

    pub fn subscribe(&self, cx: &mut ViewContext<Self>) {
        let messages = self.messages.clone();

        cx.observe_global::<ChatRegistry>(move |_, cx| {
            let state = cx.global::<ChatRegistry>();
            let events = state.new_messages.clone();
            // let mut metadata = state.metadata.clone();

            // TODO: filter messages
            let items: Vec<MessageItem> = events
                .into_iter()
                .map(|m| {
                    MessageItem::new(
                        m.event.pubkey,
                        m.metadata,
                        m.event.content,
                        m.event.created_at,
                    )
                })
                .collect();

            cx.update_model(&messages, |a, b| {
                a.items.extend(items);
                a.count = a.items.len();
                b.notify();
            });
        })
        .detach();
    }

    // TODO: support chat room
    pub fn send_message(&mut self, cx: &mut ViewContext<Self>) {
        let owner = self.owner;
        let content = self.input.read(cx).text().to_string();
        let content_clone = content.clone();

        let async_input = self.input.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();

                async move {
                    let send: anyhow::Result<(), anyhow::Error> = async_cx
                        .background_executor()
                        .spawn(async move {
                            let signer = client.signer().await?;
                            let public_key = signer.get_public_key().await?;

                            // Send message to [owner]
                            if client
                                .send_private_msg(owner, content, vec![])
                                .await
                                .is_ok()
                            {
                                // Send a copy to [yourself]
                                _ = client
                                    .send_private_msg(
                                        public_key,
                                        content_clone,
                                        vec![Tag::public_key(owner)],
                                    )
                                    .await?
                            }

                            Ok(())
                        })
                        .await;

                    if send.is_ok() {
                        _ = async_cx.update_view(&async_input, |input, cx| {
                            input.set_text("", cx);
                        });
                    }
                }
            })
            .detach();
    }
}

impl Render for ChatRoom {
    fn render(&mut self, cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(list(self.list.clone()).flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .h_12()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .child(
                        Button::new("upload")
                            .icon(Icon::new(IconName::Upload))
                            .ghost(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .bg(cx.theme().muted)
                            .rounded(px(cx.theme().radius))
                            .px_2()
                            .child(self.input.clone()),
                    ),
            )
    }
}
