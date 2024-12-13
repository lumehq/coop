use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::cmp::Reverse;

use crate::get_client;

pub struct ChatRegistry {
    events: Model<Option<Vec<Event>>>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let events = cx.new_model(|_| None);

        cx.set_global(Self::new(events));
    }

    pub fn load(&self, cx: &mut AppContext) {
        let mut async_cx = cx.to_async();
        let async_events = self.events.clone();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();
                let public_key = signer.get_public_key().await.unwrap();

                let filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .pubkey(public_key);

                let events = async_cx
                    .background_executor()
                    .spawn(async move {
                        if let Ok(events) = client.database().query(vec![filter]).await {
                            events
                                .into_iter()
                                .filter(|ev| ev.pubkey != public_key) // Filter messages from current user
                                .unique_by(|ev| ev.pubkey) // Get unique list
                                .sorted_by_key(|ev| Reverse(ev.created_at)) // Sort by created at
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        }
                    })
                    .await;

                async_cx.update_model(&async_events, |a, b| {
                    *a = Some(events);
                    b.notify();
                })
            })
            .detach();
    }

    pub fn push(&self, event: Event, cx: &mut AppContext) {
        cx.update_model(&self.events, |a, b| {
            if let Some(events) = a {
                events.push(event);
                b.notify();
            }
        })
    }

    pub fn get(&self, cx: &WindowContext) -> Option<Vec<Event>> {
        self.events.read(cx).clone()
    }

    fn new(events: Model<Option<Vec<Event>>>) -> Self {
        Self { events }
    }
}
