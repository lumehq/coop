use std::{cmp::Reverse, collections::HashMap};

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
    loading: bool,
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
                        this.update(cx, |this, cx| {
                            this.update_members(profiles, cx);
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
            loading: true,
            subscriptions,
        }
    }

    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = get_client();
        let room_ids = self.room_ids(cx);

        type LoadResult = Result<Vec<(Event, usize, bool)>, Error>;

        let task: Task<LoadResult> = cx.background_spawn(async move {
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

            let mut room_map: HashMap<u64, (Event, usize, bool)> = HashMap::new();

            for event in events
                .into_iter()
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
            {
                let hash = room_hash(&event);

                if !room_ids.iter().any(|id| id == &hash) {
                    let filter = Filter::new().kind(Kind::ContactList).pubkey(event.pubkey);
                    let is_trust = client.database().count(filter).await? >= 1;

                    room_map
                        .entry(hash)
                        .and_modify(|(_, count, trusted)| {
                            *count += 1;
                            *trusted = is_trust;
                        })
                        .or_insert((event, 1, is_trust));
                }
            }

            let result: Vec<(Event, usize, bool)> = room_map
                .into_values()
                .sorted_by_key(|(ev, _, _)| Reverse(ev.created_at))
                .collect();

            Ok(result)
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(events) = task.await {
                let rooms: Vec<Entity<Room>> = events
                    .into_iter()
                    .map(|(event, count, trusted)| {
                        let kind = if count > 2 {
                            // If frequency count is greater than 2, mark this room as ongoing
                            RoomKind::Ongoing
                        } else if trusted {
                            RoomKind::Trusted
                        } else {
                            RoomKind::Unknown
                        };

                        cx.new(|_| Room::new(&event, kind)).unwrap()
                    })
                    .collect();

                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        this.rooms.extend(rooms);
                        this.rooms.sort_by_key(|r| Reverse(r.read(cx).last_seen()));
                        this.loading = false;

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
    pub fn room_ids(&self, cx: &mut Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }

    /// Get all rooms.
    pub fn rooms(&self, cx: &App) -> HashMap<RoomKind, Vec<&Entity<Room>>> {
        let mut groups = HashMap::new();
        groups.insert(RoomKind::Ongoing, Vec::new());
        groups.insert(RoomKind::Trusted, Vec::new());
        groups.insert(RoomKind::Unknown, Vec::new());

        for room in self.rooms.iter() {
            let kind = room.read(cx).kind();
            groups.entry(kind).or_insert_with(Vec::new).push(room);
        }

        groups
    }

    /// Get rooms by their kind.
    pub fn rooms_by_kind(&self, kind: RoomKind, cx: &App) -> Vec<&Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).kind() == kind)
            .collect()
    }

    /// Get the loading status of the rooms.
    pub fn loading(&self) -> bool {
        self.loading
    }

    /// Get a room by its ID.
    pub fn get(&self, id: &u64, cx: &App) -> Option<Entity<Room>> {
        self.rooms
            .iter()
            .find(|model| model.read(cx).id == *id)
            .cloned()
    }

    /// Push a room to the list.
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

    /// Push a message to a room.
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
