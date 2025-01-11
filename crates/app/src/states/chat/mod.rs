use crate::{get_client, utils::room_hash};
use gpui::{AppContext, Context, Global, Model, SharedString, WeakModel};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use room::Room;
use std::{
    cmp::Reverse,
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub mod room;

#[derive(Clone, Debug)]
pub struct NewMessage {
    pub event: Event,
    pub metadata: Metadata,
}

impl NewMessage {
    pub fn new(event: Event, metadata: Metadata) -> Self {
        // TODO: parse event's content
        Self { event, metadata }
    }
}

type NewMessages = RwLock<HashMap<SharedString, Arc<RwLock<Vec<NewMessage>>>>>;

pub struct ChatRegistry {
    inbox: Model<Vec<Model<Room>>>,
    new_messages: Model<NewMessages>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let inbox = cx.new_model(|_| Vec::new());
        let new_messages = cx.new_model(|_| RwLock::new(HashMap::new()));

        cx.observe_new_models::<Room>(|this, cx| {
            // Get all pubkeys to load metadata
            let pubkeys: Vec<PublicKey> = this.members.iter().map(|m| m.public_key()).collect();

            cx.spawn(|weak_model, mut async_cx| async move {
                let query: Result<Vec<(PublicKey, Metadata)>, Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let client = get_client();
                        let mut profiles = Vec::new();

                        for public_key in pubkeys.into_iter() {
                            let query = client.database().metadata(public_key).await?;
                            let metadata = query.unwrap_or_default();

                            profiles.push((public_key, metadata));
                        }

                        Ok(profiles)
                    })
                    .await;

                if let Ok(profiles) = query {
                    if let Some(model) = weak_model.upgrade() {
                        _ = async_cx.update_model(&model, |model, cx| {
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

        cx.set_global(Self {
            inbox,
            new_messages,
        });
    }

    pub fn init(&mut self, cx: &mut AppContext) {
        let mut async_cx = cx.to_async();
        let async_inbox = self.inbox.clone();

        // Get all current room's id
        let hashes: Vec<u64> = self
            .inbox
            .read(cx)
            .iter()
            .map(|room| room.read(cx).id)
            .collect();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let query: anyhow::Result<Vec<Event>, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let signer = client.signer().await?;
                        let public_key = signer.get_public_key().await?;

                        let filter = Filter::new()
                            .kind(Kind::PrivateDirectMessage)
                            .author(public_key);

                        // Get all DM events from database
                        let events = client.database().query(vec![filter]).await?;

                        // Filter result
                        // - Only unique rooms
                        // - Sorted by created_at
                        let result = events
                            .into_iter()
                            .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                            .unique_by(|ev| room_hash(&ev.tags))
                            .sorted_by_key(|ev| Reverse(ev.created_at))
                            .collect::<Vec<_>>();

                        Ok(result)
                    })
                    .await;

                if let Ok(events) = query {
                    _ = async_cx.update_model(&async_inbox, |model, cx| {
                        let items: Vec<Model<Room>> = events
                            .into_iter()
                            .filter_map(|ev| {
                                let id = room_hash(&ev.tags);
                                // Filter all seen events
                                if !hashes.iter().any(|h| h == &id) {
                                    Some(cx.new_model(|_| Room::new(&ev)))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        model.extend(items);
                        cx.notify();
                    });
                }
            })
            .detach();
    }

    pub fn inbox(&self) -> WeakModel<Vec<Model<Room>>> {
        self.inbox.downgrade()
    }

    pub fn new_messages(&self) -> WeakModel<NewMessages> {
        self.new_messages.downgrade()
    }

    pub fn receive(&mut self, event: Event, metadata: Metadata, cx: &mut AppContext) {
        let entry = room_hash(&event.tags).to_string().into();
        let message = NewMessage::new(event, metadata);

        self.new_messages.update(cx, |this, cx| {
            this.write()
                .unwrap()
                .entry(entry)
                .or_insert(Arc::new(RwLock::new(Vec::new())))
                .write()
                .unwrap()
                .push(message);

            cx.notify();
        })
    }
}
