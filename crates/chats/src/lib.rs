use std::collections::BTreeSet;
use std::{cmp::Reverse, collections::HashMap};

use anyhow::Error;
use common::room_hash;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use global::shared_state;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, Global, Subscription, Task, WeakEntity, Window,
};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::RoomKind;
use smallvec::{smallvec, SmallVec};

use crate::room::Room;

pub mod message;
pub mod room;

mod constants;

pub fn init(cx: &mut App) {
    ChatRegistry::set_global(cx.new(ChatRegistry::new), cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

#[derive(Debug)]
pub enum RoomEmitter {
    Open(WeakEntity<Room>),
    Request(RoomKind),
}

/// Main registry for managing chat rooms and user profiles
///
/// The ChatRegistry is responsible for:
/// - Managing chat rooms and their states
/// - Tracking user profiles
/// - Loading room data from the lmdb
/// - Handling messages and room creation
pub struct ChatRegistry {
    /// Collection of all chat rooms
    pub rooms: Vec<Entity<Room>>,

    /// Indicates if rooms are currently being loaded
    ///
    /// Always equal to `true` when the app starts
    pub loading: bool,

    /// Subscriptions for observing changes
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl EventEmitter<RoomEmitter> for ChatRegistry {}

impl ChatRegistry {
    /// Retrieve the Global ChatRegistry instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalChatRegistry>().0.clone()
    }

    /// Retrieve the ChatRegistry instance
    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalChatRegistry>().0.read(cx)
    }

    /// Set the global ChatRegistry instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalChatRegistry(state));
    }

    /// Create a new ChatRegistry instance
    fn new(cx: &mut Context<Self>) -> Self {
        let mut subscriptions = smallvec![];

        // When any Room is created, load metadata for all members
        subscriptions.push(cx.observe_new::<Room>(|this, _window, cx| {
            this.load_metadata(cx).detach();
        }));

        Self {
            rooms: vec![],
            loading: true,
            subscriptions,
        }
    }

    /// Get a room by its ID.
    pub fn room(&self, id: &u64, cx: &App) -> Option<Entity<Room>> {
        self.rooms
            .iter()
            .find(|model| model.read(cx).id == *id)
            .cloned()
    }

    /// Get all ongoing rooms.
    pub fn ongoing_rooms(&self, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).kind == RoomKind::Ongoing)
            .cloned()
            .collect()
    }

    /// Get all request rooms.
    pub fn request_rooms(&self, trusted_only: bool, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| {
                if trusted_only {
                    room.read(cx).kind == RoomKind::Trusted
                } else {
                    room.read(cx).kind != RoomKind::Ongoing
                }
            })
            .cloned()
            .collect()
    }

    /// Sort rooms by their created at.
    pub fn sort(&mut self, cx: &mut Context<Self>) {
        self.rooms.sort_by_key(|ev| Reverse(ev.read(cx).created_at));
        cx.notify();
    }

    /// Search rooms by their name.
    pub fn search(&self, query: &str, cx: &App) -> Vec<Entity<Room>> {
        let matcher = SkimMatcherV2::default();

        self.rooms
            .iter()
            .filter(|room| {
                matcher
                    .fuzzy_match(room.read(cx).display_name(cx).as_ref(), query)
                    .is_some()
            })
            .cloned()
            .collect()
    }

    /// Search rooms by public keys.
    pub fn search_by_public_key(&self, public_key: PublicKey, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).members.contains(&public_key))
            .cloned()
            .collect()
    }

    /// Load all rooms from the lmdb.
    ///
    /// This method:
    /// 1. Fetches all private direct messages from the lmdb
    /// 2. Groups them by ID
    /// 3. Determines each room's type based on message frequency and trust status
    /// 4. Creates Room entities for each unique room
    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Starting to load rooms from database...");

        let task: Task<Result<BTreeSet<Room>, Error>> = cx.background_spawn(async move {
            let client = shared_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Get messages sent by the user
            let send = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key);

            // Get messages received by the user
            let recv = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(public_key);

            let send_events = client.database().query(send).await?;
            let recv_events = client.database().query(recv).await?;
            let events = send_events.merge(recv_events);

            let mut rooms: BTreeSet<Room> = BTreeSet::new();
            let mut trusted_keys: BTreeSet<PublicKey> = BTreeSet::new();

            // Process each event and group by room hash
            for event in events
                .into_iter()
                .sorted_by_key(|event| Reverse(event.created_at))
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
            {
                let hash = room_hash(&event);

                if rooms.iter().any(|room| room.id == hash) {
                    continue;
                }

                let mut public_keys = event.tags.public_keys().copied().collect_vec();
                public_keys.push(event.pubkey);

                let mut is_trust = trusted_keys.contains(&event.pubkey);

                if !is_trust {
                    // Check if room's author is seen in any contact list
                    let filter = Filter::new().kind(Kind::ContactList).pubkey(event.pubkey);
                    // If room's author is seen at least once, mark as trusted
                    is_trust = client.database().count(filter).await.unwrap_or(0) >= 1;

                    if is_trust {
                        trusted_keys.insert(event.pubkey);
                    }
                }

                // Check if current_user has sent a message to this room at least once
                let filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(public_key)
                    .pubkeys(public_keys);

                // If current user has sent a message at least once, mark as ongoing
                let is_ongoing = client.database().count(filter).await.unwrap_or(1) >= 1;

                if is_ongoing {
                    rooms.insert(Room::new(&event).kind(RoomKind::Ongoing));
                } else if is_trust {
                    rooms.insert(Room::new(&event).kind(RoomKind::Trusted));
                } else {
                    rooms.insert(Room::new(&event));
                }
            }

            Ok(rooms)
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(rooms) => {
                    this.update(cx, |this, cx| {
                        this.extend_rooms(rooms, cx);
                        this.sort(cx);
                    })
                    .ok();
                }
                Err(e) => {
                    // TODO: push notification
                    log::error!("Failed to load rooms: {e}")
                }
            };
        })
        .detach();
    }

    pub(crate) fn extend_rooms(&mut self, rooms: BTreeSet<Room>, cx: &mut Context<Self>) {
        let mut room_map: HashMap<u64, usize> = HashMap::with_capacity(self.rooms.len());

        for (index, room) in self.rooms.iter().enumerate() {
            room_map.insert(room.read(cx).id, index);
        }

        for new_room in rooms.into_iter() {
            // Check if we already have a room with this ID
            if let Some(&index) = room_map.get(&new_room.id) {
                self.rooms[index].update(cx, |this, cx| {
                    *this = new_room;
                    cx.notify();
                });
            } else {
                let new_index = self.rooms.len();
                room_map.insert(new_room.id, new_index);
                self.rooms.push(cx.new(|_| new_room));
            }
        }
    }

    /// Push a new Room to the global registry
    pub fn push_room(&mut self, room: Entity<Room>, cx: &mut Context<Self>) {
        let weak_room = if let Some(room) = self
            .rooms
            .iter()
            .find(|this| this.read(cx).id == room.read(cx).id)
        {
            room.downgrade()
        } else {
            let weak_room = room.downgrade();

            // Add this room to the global registry
            self.rooms.insert(0, room);
            cx.notify();

            weak_room
        };

        cx.emit(RoomEmitter::Open(weak_room));
    }

    /// Parse a Nostr event into a Coop Message and push it to the belonging room
    ///
    /// If the room doesn't exist, it will be created.
    /// Updates room ordering based on the most recent messages.
    pub fn event_to_message(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let id = room_hash(&event);
        let author = event.pubkey;
        let Some(public_key) = Identity::get_global(cx).profile().map(|i| i.public_key()) else {
            return;
        };

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            // Update room
            room.update(cx, |this, cx| {
                this.created_at(event.created_at, cx);

                // Set this room is ongoing if the new message is from current user
                if author == public_key {
                    this.set_ongoing(cx);
                }

                // Emit the new message to the room
                cx.defer_in(window, |this, window, cx| {
                    this.emit_message(event, window, cx);
                });
            });

            // Re-sort the rooms registry by their created at
            self.sort(cx);

            cx.notify();
        } else {
            let room = Room::new(&event).kind(RoomKind::Unknown);
            let kind = room.kind;

            // Push the new room to the front of the list
            self.rooms.insert(0, cx.new(|_| room));

            cx.emit(RoomEmitter::Request(kind));
            cx.notify();
        }
    }

    pub fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.loading = status;
        cx.notify();
    }
}
