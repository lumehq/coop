use std::{cmp::Reverse, collections::HashMap, sync::Arc};

use anyhow::anyhow;
use common::{last_seen::LastSeen, utils::room_hash};
use global::get_client;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::RoomKind;
use smallvec::{smallvec, SmallVec};

use crate::room::Room;

pub mod message;
pub mod room;

pub fn init(cx: &mut App) {
    ChatRegistry::set_global(cx.new(ChatRegistry::new), cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

pub struct ChatRegistry {
    rooms: Vec<Entity<Room>>,
    is_loading: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl ChatRegistry {
    pub fn global(cx: &mut App) -> Entity<Self> {
        cx.global::<GlobalChatRegistry>().0.clone()
    }

    pub fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalChatRegistry(state));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let mut subscriptions = smallvec![];

        subscriptions.push(cx.observe_new::<Room>(|this, _, cx| {
            let load_metadata = this.load_metadata(cx);

            cx.spawn(async move |this, cx| {
                if let Ok(profiles) = load_metadata.await {
                    cx.update(|cx| {
                        this.update(cx, |this: &mut Room, cx| {
                            // Update the room's name if it's not already set
                            if this.name.is_none() {
                                let mut name = profiles
                                    .iter()
                                    .take(2)
                                    .map(|profile| profile.name.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ");

                                if profiles.len() > 2 {
                                    name = format!("{}, +{}", name, profiles.len() - 2);
                                }

                                this.name = Some(name.into())
                            };

                            // Extend the room's members with the new profiles
                            let mut new_members = SmallVec::new();
                            new_members.extend(profiles);
                            this.members = Arc::new(new_members);

                            cx.notify();
                        })
                        .ok();
                    })
                    .ok();
                }
            })
            .detach();
        }));

        Self {
            rooms: vec![],
            is_loading: true,
            subscriptions,
        }
    }

    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = get_client();

        let task: Task<Result<Vec<(Event, usize)>, Error>> = cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let send = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key);

            let recv = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(public_key);

            let send_events = client.database().query(send).await?;
            let recv_events = client.database().query(recv).await?;
            let events = send_events.merge(recv_events);

            let mut room_counts: HashMap<u64, (Event, usize)> = HashMap::new();

            for event in events
                .into_iter()
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
            {
                let hash = room_hash(&event);
                room_counts
                    .entry(hash)
                    .and_modify(|(_, count)| *count += 1)
                    .or_insert((event, 1));
            }

            let result: Vec<(Event, usize)> = room_counts
                .into_values()
                .sorted_by_key(|(ev, _)| Reverse(ev.created_at))
                .collect();

            Ok(result)
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(events) = task.await {
                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        let current_ids = this.rooms_ids(cx);
                        let rooms: Vec<Entity<Room>> = events
                            .into_iter()
                            .filter_map(|item| {
                                let new = room_hash(&item.0);
                                // Filter all seen rooms
                                if !current_ids.iter().any(|this| this == &new) {
                                    Some(cx.new(|_| {
                                        // If frequency is greater than 2, mark this room as inbox
                                        let kind = if item.1 > 2 {
                                            RoomKind::Inbox
                                        } else {
                                            RoomKind::Others
                                        };
                                        Room::new(&item.0, kind)
                                    }))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        this.is_loading = false;
                        this.rooms.extend(rooms);
                        this.rooms
                            .sort_by_key(|room| Reverse(room.read(cx).last_seen()));

                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Get the IDs of all rooms.
    pub fn rooms_ids(&self, cx: &mut Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }

    /// Get all rooms.
    pub fn rooms(&self) -> &[Entity<Room>] {
        &self.rooms
    }

    /// Get the loading status of the rooms.
    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    /// Get a room by its ID.
    pub fn get(&self, id: &u64, cx: &App) -> Option<Entity<Room>> {
        self.rooms
            .iter()
            .find(|model| model.read(cx).id == *id)
            .cloned()
    }

    pub fn push(&mut self, room: Room, cx: &mut Context<Self>) -> Result<(), anyhow::Error> {
        let room = cx.new(|_| room);

        if !self
            .rooms
            .iter()
            .any(|current| current.read(cx) == room.read(cx))
        {
            self.rooms.insert(0, room);
            cx.notify();

            Ok(())
        } else {
            Err(anyhow!("Room already exists"))
        }
    }

    pub fn push_message(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let id = room_hash(&event);

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            room.update(cx, |this, cx| {
                this.set_last_seen(LastSeen(event.created_at), cx);
                this.emit_message(event, window, cx);
            });

            // Re-sort rooms by last seen
            self.rooms
                .sort_by_key(|room| Reverse(room.read(cx).last_seen()));
        } else {
            let new_room = cx.new(|_| Room::new(&event, RoomKind::default()));

            // Push the new room to the front of the list
            self.rooms.insert(0, new_room);
        }

        cx.notify();
    }
}
