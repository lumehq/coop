use std::{cmp::Reverse, rc::Rc, sync::RwLock};

use anyhow::anyhow;
use common::{last_seen::LastSeen, utils::room_hash};
use global::get_client;
use gpui::{App, AppContext, Context, Entity, Global, Task, WeakEntity};
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::room::{IncomingEvent, Room};

pub fn init(cx: &mut App) {
    ChatRegistry::register(cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

pub struct ChatRegistry {
    rooms: Rc<RwLock<Vec<Entity<Room>>>>,
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

            entity
        })
    }

    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            rooms: Rc::new(RwLock::new(vec![])),
            is_loading: true,
        }
    }

    pub fn current_rooms_ids(&self, cx: &mut Context<Self>) -> Vec<u64> {
        self.rooms
            .read()
            .unwrap()
            .iter()
            .map(|room| room.read(cx).id)
            .collect()
    }

    pub fn load_chat_rooms(&mut self, cx: &mut Context<Self>) {
        let client = get_client();

        let task: Task<Result<Vec<Event>, Error>> = cx.background_spawn(async move {
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

            let result: Vec<Event> = events
                .into_iter()
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                .unique_by(room_hash)
                .sorted_by_key(|ev| Reverse(ev.created_at))
                .collect();

            Ok(result)
        });

        cx.spawn(|this, cx| async move {
            if let Ok(events) = task.await {
                cx.update(|cx| {
                    if !events.is_empty() {
                        this.update(cx, |this, cx| {
                            let mut rooms = this.rooms.write().unwrap();
                            let current_ids = this.current_rooms_ids(cx);
                            let items: Vec<Entity<Room>> = events
                                .into_iter()
                                .filter_map(|ev| {
                                    let new = room_hash(&ev);
                                    // Filter all seen rooms
                                    if !current_ids.iter().any(|this| this == &new) {
                                        Some(Room::new(&ev, cx))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            rooms.extend(items);
                            rooms.sort_by_key(|room| Reverse(room.read(cx).last_seen()));
                            this.is_loading = false;

                            cx.notify();
                        })
                        .ok();
                    } else {
                        this.update(cx, |this, cx| {
                            this.is_loading = false;
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn rooms(&self) -> Vec<Entity<Room>> {
        self.rooms.read().unwrap().clone()
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn get(&self, id: &u64, cx: &App) -> Option<WeakEntity<Room>> {
        self.rooms
            .read()
            .unwrap()
            .iter()
            .find(|model| model.read(cx).id == *id)
            .map(|room| room.downgrade())
    }

    pub fn push_room(
        &mut self,
        room: Entity<Room>,
        cx: &mut Context<Self>,
    ) -> Result<(), anyhow::Error> {
        let mut rooms = self.rooms.write().unwrap();

        if !rooms
            .iter()
            .any(|current| current.read(cx) == room.read(cx))
        {
            rooms.insert(0, room);
            cx.notify();

            Ok(())
        } else {
            Err(anyhow!("Room is existed"))
        }
    }

    pub fn push_message(&mut self, event: Event, cx: &mut Context<Self>) {
        let id = room_hash(&event);
        let mut rooms = self.rooms.write().unwrap();

        if let Some(room) = rooms.iter().find(|room| room.read(cx).id == id) {
            room.update(cx, |this, cx| {
                if let Some(last_seen) = Rc::get_mut(&mut this.last_seen) {
                    *last_seen = LastSeen(event.created_at);
                }
                cx.emit(IncomingEvent { event });
                cx.notify();
            });

            // Re sort rooms by last seen
            rooms.sort_by_key(|room| Reverse(room.read(cx).last_seen()));

            cx.notify();
        } else {
            let new_room = Room::new(&event, cx);
            let mut rooms = self.rooms.write().unwrap();

            rooms.insert(0, new_room);
            cx.notify();
        }
    }
}
