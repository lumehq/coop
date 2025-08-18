use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Error;
use common::event::EventUtils;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use global::nostr_client;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, Global, Subscription, Task, WeakEntity, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::RoomKind;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};

use crate::room::Room;

pub mod message;
pub mod room;

pub fn init(cx: &mut App) {
    Registry::set_global(cx.new(Registry::new), cx);
}

struct GlobalRegistry(Entity<Registry>);

impl Global for GlobalRegistry {}

#[derive(Debug)]
pub enum RegistrySignal {
    Open(WeakEntity<Room>),
    Close(u64),
    NewRequest(RoomKind),
}

/// Main registry for managing chat rooms and user profiles
pub struct Registry {
    /// Collection of all chat rooms
    pub rooms: Vec<Entity<Room>>,

    /// Collection of all persons (user profiles)
    pub persons: BTreeMap<PublicKey, Entity<Profile>>,

    /// Indicates if rooms are currently being loaded
    ///
    /// Always equal to `true` when the app starts
    pub loading: bool,

    /// Subscriptions for observing changes
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 2]>,
}

impl EventEmitter<RegistrySignal> for Registry {}

impl Registry {
    /// Retrieve the Global Registry state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalRegistry>().0.clone()
    }

    /// Retrieve the Registry instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalRegistry>().0.read(cx)
    }

    /// Set the global Registry instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalRegistry(state));
    }

    /// Create a new Registry instance
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let mut subscriptions = smallvec![];

        // Load all user profiles from the database when the Registry is created
        subscriptions.push(cx.observe_new::<Self>(|this, _window, cx| {
            let task = this.load_local_person(cx);
            this.set_persons_from_task(task, cx);
        }));

        // When any Room is created, load members metadata
        subscriptions.push(cx.observe_new::<Room>(|this, _window, cx| {
            let state = Self::global(cx);
            let task = this.load_metadata(cx);

            state.update(cx, |this, cx| {
                this.set_persons_from_task(task, cx);
            });
        }));

        Self {
            rooms: vec![],
            persons: BTreeMap::new(),
            loading: true,
            subscriptions,
        }
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.rooms = vec![];
        self.loading = true;
        cx.notify();
    }

    pub(crate) fn set_persons_from_task(
        &mut self,
        task: Task<Result<Vec<Profile>, Error>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            if let Ok(profiles) = task.await {
                this.update(cx, |this, cx| {
                    for profile in profiles {
                        this.persons
                            .insert(profile.public_key(), cx.new(|_| profile));
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn load_local_person(&self, cx: &App) -> Task<Result<Vec<Profile>, Error>> {
        cx.background_spawn(async move {
            let filter = Filter::new().kind(Kind::Metadata).limit(100);
            let events = nostr_client().database().query(filter).await?;
            let mut profiles = vec![];

            for event in events.into_iter() {
                let metadata = Metadata::from_json(event.content).unwrap_or_default();
                let profile = Profile::new(event.pubkey, metadata);
                profiles.push(profile);
            }

            Ok(profiles)
        })
    }

    pub fn get_person(&self, public_key: &PublicKey, cx: &App) -> Profile {
        self.persons
            .get(public_key)
            .map(|e| e.read(cx))
            .cloned()
            .unwrap_or(Profile::new(public_key.to_owned(), Metadata::default()))
    }

    pub fn get_group_person(&self, public_keys: &[PublicKey], cx: &App) -> Vec<Profile> {
        let mut profiles = vec![];

        for public_key in public_keys.iter() {
            let profile = self.get_person(public_key, cx);
            profiles.push(profile);
        }

        profiles
    }

    pub fn insert_or_update_person(&mut self, event: Event, cx: &mut App) {
        let public_key = event.pubkey;
        let Ok(metadata) = Metadata::from_json(event.content) else {
            // Invalid metadata, no need to process further.
            return;
        };

        if let Some(person) = self.persons.get(&public_key) {
            person.update(cx, |this, cx| {
                *this = Profile::new(public_key, metadata);
                cx.notify();
            });
        } else {
            self.persons
                .insert(public_key, cx.new(|_| Profile::new(public_key, metadata)));
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
    pub fn request_rooms(&self, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).kind != RoomKind::Ongoing)
            .cloned()
            .collect()
    }

    /// Add a new room to the start of list.
    pub fn add_room(&mut self, room: Entity<Room>, cx: &mut Context<Self>) {
        self.rooms.insert(0, room);
        cx.notify();
    }

    /// Close a room.
    pub fn close_room(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.rooms.iter().any(|r| r.read(cx).id == id) {
            cx.emit(RegistrySignal::Close(id));
        }
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

    /// Set the loading status of the registry.
    pub fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.loading = status;
        cx.notify();
    }

    /// Load all rooms from the database.
    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Starting to load chat rooms...");

        // Get the contact bypass setting
        let contact_bypass = AppSettings::get_contact_bypass(cx);

        let task: Task<Result<BTreeSet<Room>, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
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

            // Process each event and group by room hash
            for event in events
                .into_iter()
                .sorted_by_key(|event| Reverse(event.created_at))
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
            {
                if rooms.iter().any(|room| room.id == event.uniq_id()) {
                    continue;
                }

                // Get all public keys from the event
                let public_keys = event.all_pubkeys();

                // Bypass screening flag
                let mut bypass = false;

                // If user enabled bypass screening for contacts
                // Check if room's members are in contact with current user
                if contact_bypass {
                    let contacts = client.database().contacts_public_keys(public_key).await?;
                    bypass = public_keys.iter().any(|k| contacts.contains(k));
                }

                // Check if the current user has sent at least one message to this room
                let filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(public_key)
                    .pubkeys(public_keys);

                // If current user has sent a message at least once, mark as ongoing
                let is_ongoing = client.database().count(filter).await.unwrap_or(1) >= 1;

                // Create a new room
                let room = Room::new(&event).rearrange_by(public_key);

                if is_ongoing || bypass {
                    rooms.insert(room.kind(RoomKind::Ongoing));
                } else {
                    rooms.insert(room);
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
        let other_id = room.read(cx).id;
        let find_room = self.rooms.iter().find(|this| this.read(cx).id == other_id);

        let weak_room = if let Some(room) = find_room {
            room.downgrade()
        } else {
            let weak_room = room.downgrade();
            // Add this room to the registry
            self.add_room(room, cx);

            weak_room
        };

        cx.emit(RegistrySignal::Open(weak_room));
    }

    /// Refresh messages for a room in the global registry
    pub fn refresh_rooms(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        for room in self.rooms.iter() {
            if ids.contains(&room.read(cx).id) {
                room.update(cx, |this, cx| {
                    this.emit_refresh(cx);
                });
            }
        }
    }

    /// Parse a Nostr event into a Coop Message and push it to the belonging room
    ///
    /// If the room doesn't exist, it will be created.
    /// Updates room ordering based on the most recent messages.
    pub fn event_to_message(
        &mut self,
        identity: PublicKey,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = event.uniq_id();
        let author = event.pubkey;

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            // Update room
            room.update(cx, |this, cx| {
                this.created_at(event.created_at, cx);

                // Set this room is ongoing if the new message is from current user
                if author == identity {
                    this.set_ongoing(cx);
                }

                // Emit the new message to the room
                cx.defer_in(window, move |this, _window, cx| {
                    this.emit_message(event, cx);
                });
            });

            // Re-sort the rooms registry by their created at
            self.sort(cx);
        } else {
            let room = Room::new(&event)
                .kind(RoomKind::default())
                .rearrange_by(identity);

            // Push the new room to the front of the list
            self.add_room(cx.new(|_| room), cx);

            // Notify the UI about the new room
            cx.defer_in(window, move |_this, _window, cx| {
                cx.emit(RegistrySignal::NewRequest(RoomKind::default()));
            });
        }
    }
}
