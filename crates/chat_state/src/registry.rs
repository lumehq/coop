use anyhow::Error;
use common::utils::{compare, room_hash};
use gpui::{App, AppContext, Entity, Global, WeakEntity, Window};
use nostr_sdk::prelude::*;
use state::get_client;

use crate::{inbox::Inbox, room::Room};

pub struct ChatRegistry {
    inbox: Entity<Inbox>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut App) {
        let inbox = cx.new(|_| Inbox::default());

        cx.observe_new::<Room>(|this, _window, cx| {
            // Get all pubkeys to load metadata
            let pubkeys = this.get_pubkeys();

            cx.spawn(|weak_model, mut async_cx| async move {
                let query: Result<Vec<(PublicKey, Metadata)>, Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let client = get_client();
                        let mut profiles = Vec::new();

                        for public_key in pubkeys.into_iter() {
                            let metadata = client
                                .database()
                                .metadata(public_key)
                                .await?
                                .unwrap_or_default();

                            profiles.push((public_key, metadata));
                        }

                        Ok(profiles)
                    })
                    .await;

                if let Ok(profiles) = query {
                    if let Some(model) = weak_model.upgrade() {
                        _ = async_cx.update_entity(&model, |model, cx| {
                            for profile in profiles.into_iter() {
                                model.set_metadata(profile.0, profile.1);
                            }
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
        })
        .detach();

        cx.set_global(Self { inbox });
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut App) {
        let window_handle = window.window_handle();

        self.inbox.update(cx, |this, cx| {
            let task = this.load(cx.to_async());

            cx.spawn(|this, mut cx| async move {
                if let Ok(events) = task.await {
                    _ = cx.update_window(window_handle, |_, _, cx| {
                        _ = this.update(cx, |this, cx| {
                            let current_rooms = this.get_room_ids(cx);
                            let items: Vec<Entity<Room>> = events
                                .into_iter()
                                .filter_map(|ev| {
                                    let id = room_hash(&ev.tags);
                                    // Filter all seen events
                                    if !current_rooms.iter().any(|h| h == &id) {
                                        Some(cx.new(|_| Room::parse(&ev)))
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
            })
            .detach();
        });
    }

    pub fn inbox(&self) -> WeakEntity<Inbox> {
        self.inbox.downgrade()
    }

    pub fn get_room(&self, id: &u64, cx: &App) -> Option<WeakEntity<Room>> {
        self.inbox
            .read(cx)
            .rooms
            .iter()
            .find(|model| &model.read(cx).id == id)
            .map(|model| model.downgrade())
    }

    pub fn new_room(&mut self, room: Room, cx: &mut App) {
        let room = cx.new(|_| room);

        self.inbox.update(cx, |this, cx| {
            if !this.rooms.iter().any(|r| r.read(cx) == room.read(cx)) {
                this.rooms.insert(0, room);
                cx.notify();
            }
        })
    }

    pub fn new_room_message(&mut self, event: Event, window: &mut Window, cx: &mut App) {
        let window_handle = window.window_handle();

        // Get all pubkeys from event's tags for comparision
        let mut pubkeys: Vec<_> = event.tags.public_keys().copied().collect();
        pubkeys.push(event.pubkey);

        if let Some(room) = self
            .inbox
            .read(cx)
            .rooms
            .iter()
            .find(|room| compare(&room.read(cx).get_pubkeys(), &pubkeys))
        {
            let weak_room = room.downgrade();

            cx.spawn(|mut cx| async move {
                if let Err(e) = cx.update_window(window_handle, |_, _, cx| {
                    _ = weak_room.update(cx, |this, cx| {
                        this.last_seen = event.created_at;
                        this.new_messages.push(event);

                        cx.notify();
                    });
                }) {
                    println!("Error: {}", e)
                }
            })
            .detach();
        } else {
            let room = cx.new(|_| Room::parse(&event));

            self.inbox.update(cx, |this, cx| {
                this.rooms.insert(0, room);
                cx.notify();
            });
        }
    }
}
