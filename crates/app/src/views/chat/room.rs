use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    theme::ActiveTheme,
    v_flex, Icon, IconName,
};

use super::message::RoomMessage;
use crate::{
    get_client,
    states::{
        account::AccountRegistry,
        chat::{ChatRegistry, Room},
        metadata::MetadataRegistry,
    },
};

#[derive(Clone)]
pub struct Messages {
    count: usize,
    items: Vec<RoomMessage>,
}

pub struct RoomPanel {
    id: SharedString,
    owner: PublicKey,
    members: Arc<[PublicKey]>,
    // Form
    input: View<TextInput>,
    // Messages
    list: ListState,
    messages: Model<Messages>,
}

impl RoomPanel {
    pub fn new(room: &Arc<Room>, cx: &mut ViewContext<'_, Self>) -> Self {
        let id = room.id.clone();
        let members: Arc<[PublicKey]> = room.members.clone().into();
        let owner = room.owner;

        // Form
        let input = cx.new_view(|cx| {
            TextInput::new(cx)
                .appearance(false)
                .text_size(ui::Size::Small)
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
            id,
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
                let owner = self.owner;
                let members = self.members.to_vec();

                let recv = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(owner)
                    .pubkeys(members.clone());

                let send = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .authors(members)
                    .pubkey(owner);

                async move {
                    let events = async_cx
                        .background_executor()
                        .spawn(async move { client.database().query(vec![recv, send]).await })
                        .await;

                    if let Ok(events) = events {
                        let items: Vec<RoomMessage> = events
                            .into_iter()
                            .sorted_by_key(|ev| ev.created_at)
                            .map(|ev| {
                                // Get user's metadata
                                let metadata = async_cx
                                    .read_global::<MetadataRegistry, _>(|state, _cx| {
                                        state.get(&ev.pubkey)
                                    })
                                    .unwrap();

                                // Return message item
                                RoomMessage::new(ev.pubkey, metadata, ev.content, ev.created_at)
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
        let room_id = self.id.clone();
        let messages = self.messages.clone();
        let current_user = cx.global::<AccountRegistry>().get().unwrap();

        cx.observe_global::<ChatRegistry>(move |_, cx| {
            let state = cx.global::<ChatRegistry>();
            let new_messages = state.new_messages.read().unwrap().clone();
            let filter = new_messages
                .into_iter()
                .filter(|m| m.room_id == room_id && m.event.pubkey != current_user)
                .collect::<Vec<_>>();

            let items: Vec<RoomMessage> = filter
                .into_iter()
                .map(|m| {
                    RoomMessage::new(
                        m.event.pubkey,
                        m.metadata,
                        m.event.content,
                        m.event.created_at,
                    )
                })
                .collect();

            cx.update_model(&messages, |model, cx| {
                model.items.extend(items);
                model.count = model.items.len();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn send_message(&mut self, cx: &mut ViewContext<Self>) {
        let owner = self.owner;
        let current_user = cx.global::<AccountRegistry>().get().unwrap();
        let content = self.input.read(cx).text().to_string();
        let content2 = content.clone();
        let content3 = content2.clone();

        let async_input = self.input.clone();
        let async_messages = self.messages.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();

                async move {
                    // Send message to all members
                    async_cx
                        .background_executor()
                        .spawn(async move { client.send_private_msg(owner, content, vec![]).await })
                        .detach();

                    // Send a copy to yourself
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            client
                                .send_private_msg(
                                    current_user,
                                    content2,
                                    vec![Tag::public_key(owner)],
                                )
                                .await
                        })
                        .detach();

                    // Create a new room message
                    let new_message: anyhow::Result<RoomMessage, anyhow::Error> = async_cx
                        .background_executor()
                        .spawn(async move {
                            let metadata = client.database().metadata(current_user).await?;
                            let created_at = Timestamp::now();
                            let message =
                                RoomMessage::new(current_user, metadata, content3, created_at);

                            Ok(message)
                        })
                        .await;

                    if let Ok(message) = new_message {
                        _ = async_cx.update_model(&async_messages, |model, cx| {
                            model.items.extend(vec![message]);
                            model.count = model.items.len();
                            cx.notify();
                        });

                        _ = async_cx.update_view(&async_input, |input, cx| {
                            input.set_text("", cx);
                        });
                    }
                }
            })
            .detach();
    }
}

impl Render for RoomPanel {
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
                            .ghost()
                            .on_click(|_, cx| {
                                let paths = cx.prompt_for_paths(PathPromptOptions {
                                    files: true,
                                    directories: false,
                                    multiple: false,
                                });

                                cx.spawn(move |_async_cx| async move {
                                    match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                                        Ok(Some(paths)) => {
                                            // TODO: upload file to blossom server
                                            println!("Paths: {:?}", paths)
                                        }
                                        Ok(None) => {}
                                        Err(_) => {}
                                    }
                                })
                                .detach();
                            }),
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
