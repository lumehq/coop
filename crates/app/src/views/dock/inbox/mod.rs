use chat::{Chat, ChatDelegate};
use components::{theme::ActiveTheme, v_flex, StyledExt};
use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::{cmp::Reverse, time::Duration};

use crate::{get_client, states::account::AccountState};

pub mod chat;

pub struct Inbox {
    label: SharedString,
    chats: Model<Option<Vec<ChatDelegate>>>,
}

impl Inbox {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let chats = cx.new_model(|_| None);
        let async_chats = chats.clone();

        if let Some(public_key) = cx.global::<AccountState>().in_use {
            let client = get_client();
            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(public_key);

            let mut async_cx = cx.to_async();

            cx.foreground_executor()
                .spawn(async move {
                    let events = async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Ok(events) = client.database().query(vec![filter]).await {
                                events
                                    .into_iter()
                                    .sorted_by_key(|ev| Reverse(ev.created_at))
                                    .filter(|ev| ev.pubkey != public_key)
                                    .unique_by(|ev| ev.pubkey)
                                    .collect::<Vec<_>>()
                            } else {
                                Vec::new()
                            }
                        })
                        .await;

                    // Get all public keys
                    let public_keys: Vec<PublicKey> =
                        events.iter().map(|event| event.pubkey).collect();

                    // Calculate total public keys
                    let total = public_keys.len();

                    // Create subscription for metadata events
                    let filter = Filter::new()
                        .kind(Kind::Metadata)
                        .authors(public_keys)
                        .limit(total);

                    let mut chats = Vec::new();
                    let mut stream = async_cx
                        .background_executor()
                        .spawn(async move {
                            client
                                .stream_events(vec![filter], Some(Duration::from_secs(15)))
                                .await
                                .unwrap()
                        })
                        .await;

                    while let Some(event) = stream.next().await {
                        // TODO: generate some random name?
                        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
                            tag.content().map(|s| s.to_string())
                        } else {
                            None
                        };

                        let metadata = Metadata::from_json(event.content).ok();
                        let chat =
                            ChatDelegate::new(title, event.pubkey, metadata, event.created_at);

                        chats.push(chat);
                    }

                    _ = async_cx.update_model(&async_chats, |a, b| {
                        *a = Some(chats);
                        b.notify();
                    });
                })
                .detach();
        }

        Self {
            label: "Inbox".into(),
            chats,
        }
    }
}

impl Render for Inbox {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div();

        if let Some(chats) = self.chats.read(cx).as_ref() {
            content = content.children(chats.iter().map(move |item| Chat::new(item.clone())))
        }

        v_flex()
            .pt_3()
            .px_2()
            .gap_2()
            .child(
                div()
                    .id("inbox")
                    .h_7()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.7))
                    .child(self.label.clone()),
            )
            .child(content)
    }
}
