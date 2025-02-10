use async_utility::tokio::sync::oneshot;
use common::utils::{compare, room_hash, signer_public_key};
use gpui::{App, AppContext, Entity, Global, WeakEntity, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use state::get_client;
use std::cmp::Reverse;

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
            let pubkeys = this.pubkeys();

            cx.spawn(|this, mut cx| async move {
                let (tx, rx) = oneshot::channel::<Vec<(PublicKey, Metadata)>>();

                cx.background_spawn(async move {
                    let client = get_client();
                    let mut profiles = Vec::new();

                    for public_key in pubkeys.into_iter() {
                        if let Ok(metadata) = client.database().metadata(public_key).await {
                            profiles.push((public_key, metadata.unwrap_or_default()));
                        }
                    }

                    _ = tx.send(profiles);
                })
                .detach();

                if let Ok(profiles) = rx.await {
                    if let Some(room) = this.upgrade() {
                        _ = cx.update_entity(&room, |this, cx| {
                            for profile in profiles.into_iter() {
                                this.set_metadata(profile.0, profile.1);
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
        let inbox = self.inbox.downgrade();

        cx.spawn(|mut cx| async move {
            let (tx, rx) = oneshot::channel::<Vec<Event>>();

            cx.background_spawn(async move {
                let client = get_client();

                if let Ok(public_key) = signer_public_key(client).await {
                    let filter = Filter::new()
                        .kind(Kind::PrivateDirectMessage)
                        .author(public_key);

                    // Get all DM events from database
                    if let Ok(events) = client.database().query(filter).await {
                        let result: Vec<Event> = events
                            .into_iter()
                            .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                            .unique_by(room_hash)
                            .sorted_by_key(|ev| Reverse(ev.created_at))
                            .collect();

                        _ = tx.send(result);
                    }
                }
            })
            .detach();

            if let Ok(events) = rx.await {
                _ = cx.update_window(window_handle, |_, _, cx| {
                    _ = inbox.update(cx, |this, cx| {
                        let current_rooms = this.ids(cx);
                        let items: Vec<Entity<Room>> = events
                            .into_iter()
                            .filter_map(|ev| {
                                let new = room_hash(&ev);
                                // Filter all seen events
                                if !current_rooms.iter().any(|this| this == &new) {
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
            .find(|room| compare(&room.read(cx).pubkeys(), &pubkeys))
        {
            let this = room.downgrade();

            cx.spawn(|mut cx| async move {
                if let Err(e) = cx.update_window(window_handle, |_, _, cx| {
                    _ = this.update(cx, |this, cx| {
                        this.last_seen.set(event.created_at);
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
