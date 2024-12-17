use flume::Receiver;
use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::get_client;

pub struct ChatRegistry {
    chats: Model<Option<Vec<Event>>>,
    is_initialized: bool,
    // Use for receive new message
    pub(crate) receiver: Receiver<Event>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext, receiver: Receiver<Event>) {
        let chats = cx.new_model(|_| None);

        cx.set_global(Self::new(chats, receiver));
    }

    pub fn load(&mut self, cx: &mut AppContext) {
        let mut async_cx = cx.to_async();
        let async_chats = self.chats.clone();

        if !self.is_initialized {
            self.is_initialized = true;

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
                                    .filter(|ev| ev.pubkey != public_key) // Filter all messages from current user
                                    .unique_by(|ev| ev.pubkey) // Get unique list
                                    .collect::<Vec<_>>()
                            } else {
                                Vec::new()
                            }
                        })
                        .await;

                    async_cx.update_model(&async_chats, |a, b| {
                        *a = Some(events);
                        b.notify();
                    })
                })
                .detach();
        }
    }

    pub fn push(&self, event: Event, cx: &mut AppContext) {
        cx.update_model(&self.chats, |a, b| {
            if let Some(chats) = a {
                if let Some(index) = chats.iter().position(|c| c.pubkey == event.pubkey) {
                    chats.swap_remove(index);
                    chats.push(event);

                    b.notify();
                } else {
                    chats.push(event);
                    b.notify();
                }
            }
        })
    }

    pub fn get(&self, cx: &WindowContext) -> Option<Vec<Event>> {
        self.chats.read(cx).clone()
    }

    fn new(chats: Model<Option<Vec<Event>>>, receiver: Receiver<Event>) -> Self {
        Self {
            chats,
            receiver,
            is_initialized: false,
        }
    }
}
