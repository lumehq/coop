use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use anyhow::Error;
use common::event::EventUtils;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use global::app_state::UnwrappingStatus;
use global::nostr_client;
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, Global, Task, WeakEntity, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::RoomKind;
use secrecy::{ExposeSecret, SecretString};
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
pub enum RegistryEvent {
    Open(WeakEntity<Room>),
    Close(u64),
    NewRequest(RoomKind),
}

/// Main registry for managing chat rooms and user profiles
pub struct Registry {
    /// Collection of all chat rooms
    pub rooms: Vec<Entity<Room>>,

    /// Collection of all persons (user profiles)
    pub persons: HashMap<PublicKey, Entity<Profile>>,

    /// Status of the unwrapping process
    pub unwrapping_status: Entity<UnwrappingStatus>,

    /// Public Key of the current user
    current_user: Option<PublicKey>,

    /// Password used for decryption secret key
    password: Entity<Option<SecretString>>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl EventEmitter<RegistryEvent> for Registry {}

impl Registry {
    /// Retrieve the global registry state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalRegistry>().0.clone()
    }

    /// Retrieve the registry instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalRegistry>().0.read(cx)
    }

    /// Set the global registry instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalRegistry(state));
    }

    /// Create a new registry instance
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let password = cx.new(|_| None);
        let unwrapping_status = cx.new(|_| UnwrappingStatus::default());
        let mut tasks = smallvec![];

        tasks.push(
            // Load all user profiles
            cx.spawn(async move |this, cx| {
                if let Ok(profiles) = Self::get_persons(cx).await {
                    this.update(cx, |this, cx| {
                        this.set_persons(profiles, cx);
                    })
                    .ok();
                }
            }),
        );

        Self {
            password,
            unwrapping_status,
            current_user: None,
            rooms: vec![],
            persons: HashMap::new(),
            _tasks: tasks,
        }
    }

    /// Create a async task to load all user profiles
    fn get_persons(cx: &AsyncApp) -> Task<Result<Vec<Profile>, Error>> {
        cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new().kind(Kind::Metadata).limit(200);
            let events = client.database().query(filter).await?;
            let mut profiles = vec![];

            for event in events.into_iter() {
                let metadata = Metadata::from_json(event.content).unwrap_or_default();
                let profile = Profile::new(event.pubkey, metadata);
                profiles.push(profile);
            }

            Ok(profiles)
        })
    }

    /// Returns the public key of the current user
    pub fn current_user(&self) -> Option<PublicKey> {
        self.current_user
    }

    /// Update the public key of the current user
    pub fn set_current_user(&mut self, public_key: PublicKey, cx: &mut Context<Self>) {
        self.current_user = Some(public_key);
        cx.notify();
    }

    /// Get the password for decryption
    pub fn password(&self, cx: &App) -> Option<String> {
        self.password
            .read(cx)
            .clone()
            .map(|pwd| pwd.expose_secret().to_owned())
    }

    /// Update the password for decryption
    pub fn set_password(&mut self, password: String, cx: &mut Context<Self>) {
        self.password.update(cx, |this, cx| {
            *this = Some(SecretString::new(Box::from(password)));
            cx.notify();
        })
    }

    /// Insert batch of persons
    pub fn set_persons(&mut self, profiles: Vec<Profile>, cx: &mut Context<Self>) {
        for profile in profiles.into_iter() {
            self.persons
                .insert(profile.public_key(), cx.new(|_| profile));
        }
        cx.notify();
    }

    /// Get single person
    pub fn get_person(&self, public_key: &PublicKey, cx: &App) -> Profile {
        self.persons
            .get(public_key)
            .map(|e| e.read(cx))
            .cloned()
            .unwrap_or(Profile::new(public_key.to_owned(), Metadata::default()))
    }

    /// Get group of persons
    pub fn get_group_person(&self, public_keys: &[PublicKey], cx: &App) -> Vec<Profile> {
        let mut profiles = vec![];

        for public_key in public_keys.iter() {
            let profile = self.get_person(public_key, cx);
            profiles.push(profile);
        }

        profiles
    }

    /// Insert or update a person
    pub fn insert_or_update_person(&mut self, profile: Profile, cx: &mut App) {
        let public_key = profile.public_key();

        match self.persons.get(&public_key) {
            Some(person) => {
                person.update(cx, |this, cx| {
                    *this = profile;
                    cx.notify();
                });
            }
            None => {
                self.persons.insert(public_key, cx.new(|_| profile));
            }
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
            cx.emit(RegistryEvent::Close(id));
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
    pub fn set_unwrapping_status(&mut self, status: UnwrappingStatus, cx: &mut Context<Self>) {
        self.unwrapping_status.update(cx, |this, cx| {
            *this = status;
            cx.notify();
        });
    }

    /// Reset the registry.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        // Reset the unwrapping status
        self.set_unwrapping_status(UnwrappingStatus::default(), cx);

        // Unset the current user
        self.current_user = None;

        // Clear all current rooms
        self.rooms.clear();

        cx.notify();
    }

    /// Load all rooms from the database.
    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Starting to load chat rooms...");

        // Get the contact bypass setting
        let bypass_setting = AppSettings::get_contact_bypass(cx);

        let task: Task<Result<HashSet<Room>, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let contacts = client.database().contacts_public_keys(public_key).await?;

            // Get messages sent by the user
            let sent = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key);

            // Get messages received by the user
            let recv = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(public_key);

            let sent_events = client.database().query(sent).await?;
            let recv_events = client.database().query(recv).await?;
            let events = sent_events.merge(recv_events);

            let mut rooms: HashSet<Room> = HashSet::new();

            // Process each event and group by room hash
            for event in events
                .into_iter()
                .sorted_by_key(|event| Reverse(event.created_at))
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
            {
                // Parse the room from the nostr event
                let room = Room::from(&event);

                // Skip if the room is already in the set
                if rooms.iter().any(|r| r.id == room.id) {
                    continue;
                }

                // Get all public keys from the event's tags
                let mut public_keys: Vec<PublicKey> = room.members().to_vec();
                public_keys.retain(|pk| pk != &public_key);

                // Bypass screening flag
                let mut bypassed = false;

                // If the user has enabled bypass screening in settings,
                // check if any of the room's members are contacts of the current user
                if bypass_setting {
                    bypassed = public_keys.iter().any(|k| contacts.contains(k));
                }

                // Check if the current user has sent at least one message to this room
                let filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(public_key)
                    .pubkeys(public_keys);

                // If current user has sent a message at least once, mark as ongoing
                let is_ongoing = client.database().count(filter).await.unwrap_or(1) >= 1;

                if is_ongoing || bypassed {
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
                    this.update_in(cx, move |_, window, cx| {
                        cx.defer_in(window, move |this, _window, cx| {
                            this.extend_rooms(rooms, cx);
                            this.sort(cx);
                        });
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

    pub(crate) fn extend_rooms(&mut self, rooms: HashSet<Room>, cx: &mut Context<Self>) {
        let mut room_map: HashMap<u64, usize> = self
            .rooms
            .iter()
            .enumerate()
            .map(|(idx, room)| (room.read(cx).id, idx))
            .collect();

        for new_room in rooms.into_iter() {
            // Check if we already have a room with this ID
            if let Some(&index) = room_map.get(&new_room.id) {
                self.rooms[index].update(cx, |this, cx| {
                    if new_room.created_at > this.created_at {
                        *this = new_room;
                        cx.notify();
                    }
                });
            } else {
                let new_room_id = new_room.id;
                self.rooms.push(cx.new(|_| new_room));

                let new_index = self.rooms.len();
                room_map.insert(new_room_id, new_index);
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

        cx.emit(RegistryEvent::Open(weak_room));
    }

    /// Refresh messages for a room in the global registry
    pub fn refresh_rooms(&mut self, ids: Option<Vec<u64>>, cx: &mut Context<Self>) {
        if let Some(ids) = ids {
            for room in self.rooms.iter() {
                if ids.contains(&room.read(cx).id) {
                    room.update(cx, |this, cx| {
                        this.emit_refresh(cx);
                    });
                }
            }
        }
    }

    /// Parse a Nostr event into a Coop Message and push it to the belonging room
    ///
    /// If the room doesn't exist, it will be created.
    /// Updates room ordering based on the most recent messages.
    pub fn event_to_message(
        &mut self,
        gift_wrap: EventId,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = event.uniq_id();
        let author = event.pubkey;

        let Some(public_key) = self.current_user() else {
            return;
        };

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            let is_new_event = event.created_at > room.read(cx).created_at;

            // Update room
            room.update(cx, |this, cx| {
                if is_new_event {
                    this.set_created_at(event.created_at, cx);
                }

                // Set this room is ongoing if the new message is from current user
                if author == public_key {
                    this.set_ongoing(cx);
                }

                // Emit the new message to the room
                cx.defer_in(window, move |this, _window, cx| {
                    this.emit_message(gift_wrap, event, cx);
                });
            });

            // Resort all rooms in the registry by their created at (after updated)
            if is_new_event {
                cx.defer_in(window, |this, _window, cx| {
                    this.sort(cx);
                });
            }
        } else {
            // Push the new room to the front of the list
            self.add_room(cx.new(|_| Room::from(&event)), cx);

            // Notify the UI about the new room
            cx.defer_in(window, move |_this, _window, cx| {
                cx.emit(RegistryEvent::NewRequest(RoomKind::default()));
            });
        }
    }
}
