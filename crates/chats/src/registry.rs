use anyhow::anyhow;
use common::utils::{compare, room_hash, signer_public_key};
use gpui::{App, AppContext, Context, Entity, Global, WeakEntity};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use state::get_client;
use std::cmp::Reverse;

use crate::room::Room;

pub fn init(cx: &mut App) {
    ChatRegistry::register(cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

pub struct ChatRegistry {
    rooms: Vec<Entity<Room>>,
    is_loading: bool,
}

impl ChatRegistry {
    pub fn global(cx: &mut App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalChatRegistry>()
            .map(|global| global.0.clone())
    }

    pub fn register(cx: &mut App) -> Entity<Self> {
        Self::global(cx).unwrap_or_else(|| {
            let entity = cx.new(|cx| {
                let mut this = Self::new(cx);
                // Automatically load chat rooms the database when the registry is created
                this.load_chat_rooms(cx);

                this
            });

            // Set global state
            cx.set_global(GlobalChatRegistry(entity.clone()));

            // Observe and load metadata for any new rooms
            cx.observe_new::<Room>(|this, _window, cx| {
                let client = get_client();
                let pubkeys = this.pubkeys();
                let (tx, rx) = oneshot::channel::<Vec<(PublicKey, Metadata)>>();

                cx.background_spawn(async move {
                    let mut profiles = Vec::new();

                    for public_key in pubkeys.into_iter() {
                        if let Ok(metadata) = client.database().metadata(public_key).await {
                            profiles.push((public_key, metadata.unwrap_or_default()));
                        }
                    }

                    _ = tx.send(profiles);
                })
                .detach();

                cx.spawn(|this, mut cx| async move {
                    if let Ok(profiles) = rx.await {
                        if let Some(room) = this.upgrade() {
                            _ = cx.update_entity(&room, |this, cx| {
                                for profile in profiles.into_iter() {
                                    this.set_metadata(profile.0, profile.1);
                                }
                                cx.notify();
                            });
                        }
                    }
                })
                .detach();
            })
            .detach();

            entity
        })
    }

    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            rooms: vec![],
            is_loading: true,
        }
    }

    pub fn current_rooms_ids(&self, cx: &mut Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }

    pub fn load_chat_rooms(&mut self, cx: &mut Context<Self>) {
        let client = get_client();
        let (tx, rx) = oneshot::channel::<Vec<Event>>();

        cx.background_spawn(async move {
            if let Ok(public_key) = signer_public_key(client).await {
                let send = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(public_key);

                let recv = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .pubkey(public_key);

                let Ok(send_events) = client.database().query(send).await else {
                    return;
                };

                let Ok(recv_events) = client.database().query(recv).await else {
                    return;
                };

                let result: Vec<Event> = send_events
                    .merge(recv_events)
                    .into_iter()
                    .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                    .unique_by(room_hash)
                    .sorted_by_key(|ev| Reverse(ev.created_at))
                    .collect();

                _ = tx.send(result);
            }
        })
        .detach();

        cx.spawn(|this, cx| async move {
            if let Ok(events) = rx.await {
                if !events.is_empty() {
                    _ = cx.update(|cx| {
                        _ = this.update(cx, |this, cx| {
                            let current_rooms = this.current_rooms_ids(cx);
                            let items: Vec<Entity<Room>> = events
                                .into_iter()
                                .filter_map(|ev| {
                                    let new = room_hash(&ev);
                                    // Filter all seen events
                                    if !current_rooms.iter().any(|this| this == &new) {
                                        Some(cx.new(|cx| Room::parse(&ev, cx)))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            this.rooms.extend(items);
                            this.is_loading = false;

                            cx.notify();
                        });
                    });
                }
            }
        })
        .detach();
    }

    pub fn rooms(&self) -> &Vec<Entity<Room>> {
        &self.rooms
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn get(&self, id: &u64, cx: &App) -> Option<WeakEntity<Room>> {
        self.rooms
            .iter()
            .find(|model| model.read(cx).id == *id)
            .map(|room| room.downgrade())
    }

    pub fn push_room(&mut self, room: Room, cx: &mut Context<Self>) -> Result<(), anyhow::Error> {
        if !self
            .rooms
            .iter()
            .any(|current| compare(&current.read(cx).pubkeys(), &room.pubkeys()))
        {
            self.rooms.insert(0, cx.new(|_| room));
            cx.notify();

            Ok(())
        } else {
            Err(anyhow!("Room is existed"))
        }
    }

    pub fn push_message(&mut self, event: Event, cx: &mut Context<Self>) {
        // Get all pubkeys from event's tags for comparision
        let mut pubkeys: Vec<_> = event.tags.public_keys().copied().collect();
        pubkeys.push(event.pubkey);

        if let Some(room) = self
            .rooms
            .iter()
            .find(|room| compare(&room.read(cx).pubkeys(), &pubkeys))
        {
            room.update(cx, |this, cx| {
                this.last_seen.set(event.created_at);
                this.new_messages.update(cx, |this, cx| {
                    this.push(event);
                    cx.notify();
                });
                cx.notify();
            });

            // Re sort rooms by last seen
            self.rooms
                .sort_by_key(|room| Reverse(room.read(cx).last_seen()));

            cx.notify();
        } else {
            let room = cx.new(|cx| Room::parse(&event, cx));
            self.rooms.insert(0, room);
            cx.notify();
        }
    }
}
