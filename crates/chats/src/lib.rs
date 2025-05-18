use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
};

use account::Account;
use anyhow::Error;
use common::room_hash;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use global::get_client;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::RoomKind;
use smallvec::{smallvec, SmallVec};
use ui::ContextModal;

use crate::room::Room;

mod constants;
pub mod message;
pub mod room;

pub fn init(cx: &mut App) {
    ChatRegistry::set_global(cx.new(ChatRegistry::new), cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

/// Main registry for managing chat rooms and user profiles
///
/// The ChatRegistry is responsible for:
/// - Managing chat rooms and their states
/// - Tracking user profiles
/// - Loading room data from the lmdb
/// - Handling messages and room creation
pub struct ChatRegistry {
    /// Map of user public keys to their profile metadata
    profiles: Entity<BTreeMap<PublicKey, Option<Metadata>>>,
    /// Collection of all chat rooms
    pub rooms: Vec<Entity<Room>>,
    /// Indicates if rooms are currently being loaded
    ///
    /// Always equal to `true` when the app starts
    pub wait_for_eose: bool,
    /// Subscriptions for observing changes
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 2]>,
}

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
    pub fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalChatRegistry(state));
    }

    /// Create a new ChatRegistry instance
    fn new(cx: &mut Context<Self>) -> Self {
        let profiles = cx.new(|_| BTreeMap::new());
        let mut subscriptions = smallvec![];

        // When the ChatRegistry is created, load all rooms from the local database
        subscriptions.push(cx.observe_new::<ChatRegistry>(|this, window, cx| {
            if let Some(window) = window {
                this.load_rooms(window, cx);
            }
        }));

        // When any Room is created, load metadata for all members
        subscriptions.push(cx.observe_new::<Room>(|this, window, cx| {
            if let Some(window) = window {
                let task = this.load_metadata(cx);

                cx.spawn_in(window, async move |_, cx| {
                    if let Ok(data) = task.await {
                        cx.update(|_, cx| {
                            for (public_key, metadata) in data.into_iter() {
                                Self::global(cx).update(cx, |this, cx| {
                                    this.add_profile(public_key, metadata, cx);
                                })
                            }
                        })
                        .ok();
                    }
                })
                .detach();
            }
        }));

        Self {
            rooms: vec![],
            wait_for_eose: true,
            profiles,
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

    /// Get rooms by its kind.
    pub fn rooms_by_kind(&self, kind: RoomKind, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).kind == kind)
            .cloned()
            .collect()
    }

    /// Get the IDs of all rooms.
    pub fn room_ids(&self, cx: &mut Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
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

    /// Load all rooms from the lmdb.
    ///
    /// This method:
    /// 1. Fetches all private direct messages from the lmdb
    /// 2. Groups them by ID
    /// 3. Determines each room's type based on message frequency and trust status
    /// 4. Creates Room entities for each unique room
    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // [event] is the Nostr Event
        // [usize] is the total number of messages, used to determine an ongoing conversation
        // [bool] is used to determine if the room is trusted
        type Rooms = Vec<(Event, usize, bool)>;

        // If the user is not logged in, do nothing
        let Some(user) = Account::get_global(cx).profile_ref() else {
            return;
        };

        let client = get_client();
        let public_key = user.public_key();

        let task: Task<Result<Rooms, Error>> = cx.background_spawn(async move {
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

            let mut room_map: HashMap<u64, (Event, usize, bool)> = HashMap::new();

            // Process each event and group by room hash
            for event in events
                .into_iter()
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
            {
                let hash = room_hash(&event);

                // Check if room's author is seen in any contact list
                let filter = Filter::new().kind(Kind::ContactList).pubkey(event.pubkey);
                // If room's author is seen at least once, mark as trusted
                let is_trust = client.database().count(filter).await? >= 1;

                room_map
                    .entry(hash)
                    .and_modify(|(_, count, trusted)| {
                        *count += 1;
                        *trusted = is_trust;
                    })
                    .or_insert((event, 1, is_trust));
            }

            // Sort rooms by creation date (newest first)
            let result: Vec<(Event, usize, bool)> = room_map
                .into_values()
                .sorted_by_key(|(ev, _, _)| Reverse(ev.created_at))
                .collect();

            Ok(result)
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(events) = task.await {
                this.update(cx, |this, cx| {
                    let ids = this.room_ids(cx);
                    let rooms: Vec<Entity<Room>> = events
                        .into_iter()
                        .filter_map(|(event, count, trusted)| {
                            let hash = room_hash(&event);
                            if !ids.iter().any(|this| this == &hash) {
                                let kind = if count > 2 {
                                    // If frequency count is greater than 2, mark this room as ongoing
                                    RoomKind::Ongoing
                                } else if trusted {
                                    RoomKind::Trusted
                                } else {
                                    RoomKind::Unknown
                                };
                                Some(cx.new(|_| Room::new(&event).kind(kind)))
                            } else {
                                None
                            }
                        })
                        .collect();

                    this.rooms.extend(rooms);
                    this.wait_for_eose = false;

                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Add a user profile to the registry
    ///
    /// Only adds the profile if it doesn't already exist or is currently none
    pub fn add_profile(
        &mut self,
        public_key: PublicKey,
        metadata: Option<Metadata>,
        cx: &mut Context<Self>,
    ) {
        self.profiles.update(cx, |this, _cx| {
            this.entry(public_key)
                .and_modify(|entry| {
                    if entry.is_none() {
                        *entry = metadata.clone();
                    }
                })
                .or_insert_with(|| metadata);
        });
    }

    /// Get a user profile by public key
    pub fn profile(&self, public_key: &PublicKey, cx: &App) -> Profile {
        let metadata = if let Some(profile) = self.profiles.read(cx).get(public_key) {
            profile.clone().unwrap_or_default()
        } else {
            Metadata::default()
        };

        Profile::new(*public_key, metadata)
    }

    /// Push a Room Entity to the global registry
    ///
    /// Returns the ID of the room
    pub fn push_room(&mut self, room: Entity<Room>, cx: &mut Context<Self>) -> u64 {
        let id = room.read(cx).id;

        if !self.rooms.iter().any(|this| this.read(cx) == room.read(cx)) {
            self.rooms.insert(0, room);
            cx.notify();
        }

        id
    }

    /// Parse a Nostr event into a Coop Room and push it to the global registry
    ///
    /// Returns the ID of the new room
    pub fn event_to_room(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        let room = Room::new(event).kind(RoomKind::Ongoing);
        let id = room.id;

        if !self.rooms.iter().any(|this| this.read(cx) == &room) {
            self.rooms.insert(0, cx.new(|_| room));
            cx.notify();
        } else {
            window.push_notification("Room already exists", cx);
        }

        id
    }

    /// Parse a Nostr event into a Coop Message and push it to the belonging room
    ///
    /// If the room doesn't exist, it will be created.
    /// Updates room ordering based on the most recent messages.
    pub fn event_to_message(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let id = room_hash(&event);

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            room.update(cx, |this, cx| {
                this.created_at(event.created_at, cx);
                // Emit the new message to the room
                cx.defer_in(window, |this, window, cx| {
                    this.emit_message(event, window, cx);
                });
            });
            cx.notify();
        } else {
            // Push the new room to the front of the list
            self.rooms.insert(0, cx.new(|_| Room::new(&event)));
            cx.notify();
        }
    }
}
