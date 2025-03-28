use std::cmp::Reverse;

use anyhow::anyhow;
use common::{last_seen::LastSeen, utils::room_hash};
use global::get_client;
use gpui::{App, AppContext, Context, Entity, Global, Task, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::room::Room;

pub mod message;
pub mod room;

pub fn init(cx: &mut App) {
    ChatRegistry::set_global(cx.new(|_| ChatRegistry::new()), cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

pub struct ChatRegistry {
    rooms: Vec<Entity<Room>>,
    is_loading: bool,
}

impl ChatRegistry {
    pub fn global(cx: &mut App) -> Entity<Self> {
        cx.global::<GlobalChatRegistry>().0.clone()
    }

    pub fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalChatRegistry(state));
    }

    fn new() -> Self {
        Self {
            rooms: vec![],
            is_loading: true,
        }
    }

    pub fn current_rooms_ids(&self, cx: &mut Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }

    pub fn load_chat_rooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(events) = task.await {
                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        if !events.is_empty() {
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

                            this.is_loading = false;
                            this.rooms.extend(items);
                            this.rooms
                                .sort_by_key(|room| Reverse(room.read(cx).last_seen()));
                        } else {
                            this.is_loading = false;
                        }

                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn rooms(&self) -> &[Entity<Room>] {
        &self.rooms
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn get(&self, id: &u64, cx: &App) -> Option<Entity<Room>> {
        self.rooms
            .iter()
            .find(|model| model.read(cx).id == *id)
            .cloned()
    }

    pub fn push_room(
        &mut self,
        room: Entity<Room>,
        cx: &mut Context<Self>,
    ) -> Result<(), anyhow::Error> {
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
            let new_room = Room::new(&event, cx);

            // Push the new room to the front of the list
            self.rooms.insert(0, new_room);
        }

        cx.notify();
    }
}
