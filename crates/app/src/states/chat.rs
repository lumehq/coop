use crate::get_client;
use crate::utils::get_room_id;
use gpui::{AppContext, Context, Global, Model, SharedString};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use rnglib::{Language, RNG};
use serde::Deserialize;
use std::{
    cmp::Reverse,
    collections::HashMap,
    sync::{Arc, RwLock},
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Room {
    pub id: SharedString,
    pub owner: PublicKey,
    pub members: Vec<PublicKey>,
    pub last_seen: Timestamp,
    pub title: Option<SharedString>,
    pub metadata: Option<Metadata>,
}

impl Room {
    pub fn new(
        id: SharedString,
        owner: PublicKey,
        members: Vec<PublicKey>,
        last_seen: Timestamp,
        title: Option<SharedString>,
        metadata: Option<Metadata>,
    ) -> Self {
        Self {
            id,
            title,
            members,
            last_seen,
            owner,
            metadata,
        }
    }

    pub fn parse(event: &Event, metadata: Option<Metadata>) -> Self {
        let owner = event.pubkey;
        let last_seen = event.created_at;
        let id = SharedString::from(get_room_id(&owner, &event.tags));

        // Get all members from event's tag
        let mut members: Vec<PublicKey> = event.tags.public_keys().copied().collect();
        members.push(owner);

        // Get title from event's tag
        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            let rng = RNG::from(&Language::Roman);
            let name = rng.generate_names(2, true).join("-").to_lowercase();

            Some(name.into())
        };

        Self::new(id, owner, members, last_seen, title, metadata)
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub event: Event,
    pub metadata: Option<Metadata>,
}

impl Message {
    pub fn new(event: Event, metadata: Option<Metadata>) -> Self {
        // TODO: parse event's content
        Self { event, metadata }
    }
}

pub struct ChatRegistry {
    pub messages: RwLock<HashMap<SharedString, Arc<RwLock<Vec<Message>>>>>,
    pub rooms: Model<Vec<Event>>,
    pub is_initialized: bool,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let rooms = cx.new_model(|_| Vec::new());
        let messages = RwLock::new(HashMap::new());

        cx.set_global(Self {
            messages,
            rooms,
            is_initialized: false,
        });
    }

    pub fn init(&mut self, cx: &mut AppContext) {
        if self.is_initialized {
            return;
        }

        let async_cx = cx.to_async();
        // Get all current room's ids
        let ids: Vec<String> = self
            .rooms
            .read(cx)
            .iter()
            .map(|ev| get_room_id(&ev.pubkey, &ev.tags))
            .collect();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();
                let public_key = signer.get_public_key().await.unwrap();

                let filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .pubkey(public_key);

                let events = async_cx
                    .background_executor()
                    .spawn(async move {
                        if let Ok(events) = client.database().query(vec![filter]).await {
                            events
                                .into_iter()
                                .filter(|ev| ev.pubkey != public_key)
                                .filter(|ev| {
                                    let new_id = get_room_id(&ev.pubkey, &ev.tags);
                                    // Get new events only
                                    !ids.iter().any(|id| id == &new_id)
                                }) // Filter all messages from current user
                                .unique_by(|ev| ev.pubkey)
                                .sorted_by_key(|ev| Reverse(ev.created_at))
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        }
                    })
                    .await;

                _ = async_cx.update_global::<Self, _>(|state, cx| {
                    state.rooms.update(cx, |model, cx| {
                        model.extend(events);
                        cx.notify();
                    });

                    state.is_initialized = true;
                });
            })
            .detach();
    }

    pub fn new_message(&mut self, event: Event, metadata: Option<Metadata>) {
        // Get room id
        let room_id = SharedString::from(get_room_id(&event.pubkey, &event.tags));
        // Create message
        let message = Message::new(event, metadata);

        self.messages
            .write()
            .unwrap()
            .entry(room_id)
            .or_insert(Arc::new(RwLock::new(Vec::new())))
            .write()
            .unwrap()
            .push(message)
    }

    pub fn get_messages(&self, id: &SharedString) -> Option<Arc<RwLock<Vec<Message>>>> {
        self.messages.read().unwrap().get(id).cloned()
    }
}
