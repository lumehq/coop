use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;

use crate::{get_client, states::chat::ChatRegistry};

pub struct MessageList {
    member: PublicKey,
    messages: Model<Option<Events>>,
}

impl MessageList {
    pub fn new(from: PublicKey, cx: &mut ViewContext<'_, Self>) -> Self {
        let messages = cx.new_model(|_| None);

        Self {
            member: from,
            messages,
        }
    }

    pub fn init(&self, cx: &mut ViewContext<Self>) {
        let messages = self.messages.clone();
        let member = self.member;

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();
                let public_key = signer.get_public_key().await.unwrap();

                let recv = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(member)
                    .pubkey(public_key);

                let send = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(public_key)
                    .pubkey(member);

                let events = async_cx
                    .background_executor()
                    .spawn(async move { client.database().query(vec![recv, send]).await })
                    .await;

                if let Ok(events) = events {
                    _ = async_cx.update_model(&messages, |a, b| {
                        *a = Some(events);
                        b.notify();
                    });
                }
            })
            .detach();
    }

    pub fn subscribe(&self, cx: &mut ViewContext<Self>) {
        let messages = self.messages.clone();

        cx.observe_global::<ChatRegistry>(move |_, cx| {
            let state = cx.global::<ChatRegistry>();
            let events = state.new_messages.clone();

            cx.update_model(&messages, |a, b| {
                if let Some(m) = a {
                    m.extend(events);
                    b.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for MessageList {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .h_full()
            .flex()
            .flex_col_reverse()
            .justify_end()
            .when_some(self.messages.read(cx).as_ref(), |this, messages| {
                this.children(messages.clone().into_iter().map(|m| {
                    div()
                        .flex()
                        .flex_col()
                        .child(m.pubkey.to_hex())
                        .child(m.content)
                }))
            })
    }
}
