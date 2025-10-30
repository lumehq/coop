use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use account::Account;
use anyhow::Error;
use common::event::EventUtils;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Task, WeakEntity, Window};
use nostr_sdk::prelude::*;
use room::RoomKind;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use states::app_state;

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

pub struct Registry {
    /// Collection of all chat rooms
    pub rooms: Vec<Entity<Room>>,

    /// Loading status of the registry
    pub loading: bool,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 2]>,
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
    pub(crate) fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            rooms: vec![],
            loading: true,
            _tasks: smallvec![],
        }
    }

    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
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

    /// Reset the registry.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.rooms.clear();
        cx.notify();
    }

    /// Load all rooms from the database.
    pub fn load_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Starting to load chat rooms...");

        // Get the contact bypass setting
        let bypass_setting = AppSettings::get_contact_bypass(cx);

        let task: Task<Result<HashSet<Room>, Error>> = cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let contacts = client.database().contacts_public_keys(public_key).await?;

            let authored_filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .custom_tag(SingleLetterTag::lowercase(Alphabet::A), public_key);

            let addressed_filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .custom_tag(SingleLetterTag::lowercase(Alphabet::P), public_key);

            let authored = client.database().query(authored_filter).await?;
            let addressed = client.database().query(addressed_filter).await?;
            let events = authored.merge(addressed);

            let mut rooms: HashSet<Room> = HashSet::new();
            let mut grouped: HashMap<u64, Vec<UnsignedEvent>> = HashMap::new();

            // Process each event and group by room hash
            for raw in events.into_iter() {
                match UnsignedEvent::from_json(&raw.content) {
                    Ok(rumor) => {
                        if rumor.tags.public_keys().peekable().peek().is_some() {
                            grouped.entry(rumor.uniq_id()).or_default().push(rumor);
                        }
                    }
                    Err(e) => log::warn!("Failed to parse stored rumor: {e}"),
                }
            }

            for (_room_id, mut messages) in grouped.into_iter() {
                messages.sort_by_key(|m| Reverse(m.created_at));

                let Some(latest) = messages.first() else {
                    continue;
                };

                let mut room = Room::from(latest);

                if rooms.iter().any(|r| r.id == room.id) {
                    continue;
                }

                let mut public_keys: Vec<PublicKey> = room.members().to_vec();
                public_keys.retain(|pk| pk != &public_key);

                let user_sent = messages.iter().any(|m| m.pubkey == public_key);

                let mut bypassed = false;
                if bypass_setting {
                    bypassed = public_keys.iter().any(|k| contacts.contains(k));
                }

                if user_sent || bypassed {
                    room = room.kind(RoomKind::Ongoing);
                }

                rooms.insert(room);
            }

            Ok(rooms)
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(rooms) => {
                    this.update_in(cx, move |this, _window, cx| {
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
        event: UnsignedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = event.uniq_id();
        let author = event.pubkey;
        let account = Account::global(cx);

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            let is_new_event = event.created_at > room.read(cx).created_at;
            let created_at = event.created_at;
            let event_for_emit = event.clone();

            // Update room
            room.update(cx, |this, cx| {
                if is_new_event {
                    this.set_created_at(created_at, cx);
                }

                // Set this room is ongoing if the new message is from current user
                if author == account.read(cx).public_key() {
                    this.set_ongoing(cx);
                }

                // Emit the new message to the room
                let event_to_emit = event_for_emit.clone();
                cx.defer_in(window, move |this, _window, cx| {
                    this.emit_message(gift_wrap, event_to_emit, cx);
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
