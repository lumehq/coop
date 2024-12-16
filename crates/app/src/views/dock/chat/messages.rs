use gpui::*;
use nostr_sdk::prelude::*;

use crate::get_client;

pub struct Messages {
    messages: Model<Option<Events>>,
}

impl Messages {
    pub fn new(from: PublicKey, cx: &mut ViewContext<'_, Self>) -> Self {
        let messages = cx.new_model(|_| None);
        let async_messages = messages.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();
                let public_key = signer.get_public_key().await.unwrap();

                let recv_filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(from)
                    .pubkey(public_key);

                let sender_filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(public_key)
                    .pubkey(from);

                let events = async_cx
                    .background_executor()
                    .spawn(async move {
                        client
                            .database()
                            .query(vec![recv_filter, sender_filter])
                            .await
                    })
                    .await;

                if let Ok(events) = events {
                    _ = async_cx.update_model(&async_messages, |a, b| {
                        *a = Some(events);
                        b.notify();
                    });
                }
            })
            .detach();

        Self { messages }
    }
}

impl Render for Messages {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div().size_full().flex().flex_col().justify_end();

        if let Some(messages) = self.messages.read(cx).as_ref() {
            content = content.children(messages.clone().into_iter().map(|m| div().child(m.content)))
        }

        div().flex_1().child(content)
    }
}
