use crate::room::Room;
use anyhow::Error;
use common::utils::{compare, room_hash};
use gpui::{AppContext, AsyncAppContext, Context, Global, Model, ModelContext, Task, WeakModel};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use state::get_client;
use std::cmp::Reverse;

pub struct Inbox {
    pub rooms: Vec<Model<Room>>,
    pub is_loading: bool,
}

impl Inbox {
    pub fn new() -> Self {
        Self {
            rooms: vec![],
            is_loading: true,
        }
    }

    pub fn current_rooms(&self, cx: &ModelContext<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }

    pub fn load(&mut self, cx: AsyncAppContext) -> Task<Result<Vec<Event>, Error>> {
        cx.background_executor().spawn(async move {
            let client = get_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key);

            // Get all DM events from database
            let events = client.database().query(vec![filter]).await?;

            // Filter result
            // - Get unique rooms only
            // - Sorted by created_at
            let result = events
                .into_iter()
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                .unique_by(|ev| room_hash(&ev.tags))
                .sorted_by_key(|ev| Reverse(ev.created_at))
                .collect::<Vec<_>>();

            Ok(result)
        })
    }
}

impl Default for Inbox {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ChatRegistry {
    inbox: Model<Inbox>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let inbox = cx.new_model(|_| Inbox::default());

        cx.observe_new_models::<Room>(|this, cx| {
            // Get all pubkeys to load metadata
            let pubkeys = this.get_all_keys();

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

    pub fn load(&mut self, cx: &mut AppContext) {
        self.inbox.update(cx, |this, cx| {
            let task = this.load(cx.to_async());

            cx.spawn(|this, mut async_cx| async move {
                if let Some(inbox) = this.upgrade() {
                    if let Ok(events) = task.await {
                        _ = async_cx.update_model(&inbox, |this, cx| {
                            let current_rooms = this.current_rooms(cx);
                            let items: Vec<Model<Room>> = events
                                .into_iter()
                                .filter_map(|ev| {
                                    let id = room_hash(&ev.tags);
                                    // Filter all seen events
                                    if !current_rooms.iter().any(|h| h == &id) {
                                        Some(cx.new_model(|_| Room::parse(&ev)))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            this.rooms.extend(items);
                            this.is_loading = false;

                            cx.notify();
                        });
                    }
                }
            })
            .detach();
        });
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

    pub fn new_room(&mut self, room: Room, cx: &mut AppContext) {
        let room = cx.new_model(|_| room);

        self.inbox.update(cx, |this, cx| {
            if !this.rooms.iter().any(|r| r.read(cx) == room.read(cx)) {
                this.rooms.insert(0, room);
                cx.notify();
            }
        })
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
