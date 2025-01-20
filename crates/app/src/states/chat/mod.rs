use crate::{
    get_client,
    utils::{compare, room_hash},
};
use gpui::{AppContext, Context, Global, Model, WeakModel};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::Room;
use std::cmp::Reverse;

pub mod room;

pub struct Inbox {
    pub(crate) rooms: Vec<Model<Room>>,
    pub(crate) is_loading: bool,
}

impl Inbox {
    pub fn new() -> Self {
        Self {
            rooms: vec![],
            is_loading: true,
        }
    }
}

pub struct ChatRegistry {
    inbox: Model<Inbox>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let inbox = cx.new_model(|_| Inbox::new());

        cx.observe_new_models::<Room>(|this, cx| {
            // Get all pubkeys to load metadata
            let mut pubkeys: Vec<PublicKey> = this.members.iter().map(|m| m.public_key()).collect();
            pubkeys.push(this.owner.public_key());

            cx.spawn(|weak_model, mut async_cx| async move {
                let query: Result<Vec<(PublicKey, Metadata)>, Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let client = get_client();
                        let mut profiles = Vec::new();

                        for public_key in pubkeys.into_iter() {
                            let query = client.database().metadata(public_key).await?;
                            let metadata = query.unwrap_or_default();

                            profiles.push((public_key, metadata));
                        }

                        Ok(profiles)
                    })
                    .await;

                if let Ok(profiles) = query {
                    if let Some(model) = weak_model.upgrade() {
                        _ = async_cx.update_model(&model, |model, cx| {
                            for profile in profiles.into_iter() {
                                model.set_metadata(profile.0, profile.1);
                            }
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
        })
        .detach();

        cx.set_global(Self { inbox });
    }

    pub fn init(&mut self, cx: &mut AppContext) {
        let mut async_cx = cx.to_async();
        let async_inbox = self.inbox.clone();

        // Get all current room's id
        let hashes: Vec<u64> = self
            .inbox
            .read(cx)
            .rooms
            .iter()
            .map(|room| room.read(cx).id)
            .collect();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let query: anyhow::Result<Vec<Event>, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let signer = client.signer().await?;
                        let public_key = signer.get_public_key().await?;

                        let filter = Filter::new()
                            .kind(Kind::PrivateDirectMessage)
                            .author(public_key);

                        // Get all DM events from database
                        let events = client.database().query(vec![filter]).await?;

                        // Filter result
                        // - Only unique rooms
                        // - Sorted by created_at
                        let result = events
                            .into_iter()
                            .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                            .unique_by(|ev| room_hash(&ev.tags))
                            .sorted_by_key(|ev| Reverse(ev.created_at))
                            .collect::<Vec<_>>();

                        Ok(result)
                    })
                    .await;

                if let Ok(events) = query {
                    _ = async_cx.update_model(&async_inbox, |model, cx| {
                        let items: Vec<Model<Room>> = events
                            .into_iter()
                            .filter_map(|ev| {
                                let id = room_hash(&ev.tags);
                                // Filter all seen events
                                if !hashes.iter().any(|h| h == &id) {
                                    Some(cx.new_model(|_| Room::parse(&ev)))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        model.rooms.extend(items);
                        model.is_loading = false;

                        cx.notify();
                    });
                }
            })
            .detach();
    }

    pub fn inbox(&self) -> WeakModel<Inbox> {
        self.inbox.downgrade()
    }

    pub fn room(&self, id: &u64, cx: &AppContext) -> Option<WeakModel<Room>> {
        self.inbox
            .read(cx)
            .rooms
            .iter()
            .find(|model| &model.read(cx).id == id)
            .map(|model| model.downgrade())
    }

    pub fn receive(&mut self, event: Event, cx: &mut AppContext) {
        let mut pubkeys: Vec<_> = event.tags.public_keys().copied().collect();
        pubkeys.push(event.pubkey);

        self.inbox.update(cx, |this, cx| {
            if let Some(room) = this.rooms.iter().find(|room| {
                let all_keys = room.read(cx).get_all_keys();
                compare(&all_keys, &pubkeys)
            }) {
                room.update(cx, |this, cx| {
                    this.new_messages.push(event);
                    cx.notify();
                })
            } else {
                let room = cx.new_model(|_| Room::parse(&event));

                self.inbox.update(cx, |this, cx| {
                    this.rooms.insert(0, room);
                    cx.notify();
                })
            }

            // cx.notify();
        })
    }
}
