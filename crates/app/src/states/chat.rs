use crate::get_client;
use crate::utils::get_room_id;
use gpui::{AppContext, Context, Global, Model, SharedString, WeakModel};
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

type Messages = RwLock<HashMap<SharedString, Arc<RwLock<Vec<Message>>>>>;

pub struct ChatRegistry {
    messages: Model<Messages>,
    rooms: Model<Vec<Event>>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let rooms = cx.new_model(|_| Vec::new());
        let messages = cx.new_model(|_| RwLock::new(HashMap::new()));

        cx.set_global(Self { messages, rooms });
    }

    pub fn init(&mut self, cx: &mut AppContext) {
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
                let query: anyhow::Result<Vec<Event>, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let signer = client.signer().await?;
                        let public_key = signer.get_public_key().await?;

                        let filter = Filter::new()
                            .kind(Kind::PrivateDirectMessage)
                            .pubkey(public_key);

                        let events = client.database().query(vec![filter]).await?;
                        let result = events
                            .into_iter()
                            .filter(|ev| ev.pubkey != public_key)
                            .filter(|ev| {
                                let new_id = get_room_id(&ev.pubkey, &ev.tags);
                                // Get new events only
                                !ids.iter().any(|id| id == &new_id)
                            }) // Filter all messages from current user
                            .unique_by(|ev| ev.pubkey)
                            .sorted_by_key(|ev| Reverse(ev.created_at))
                            .collect::<Vec<_>>();

                        Ok(result)
                    })
                    .await;

                if let Ok(events) = query {
                    _ = async_cx.update_global::<Self, _>(|state, cx| {
                        state.rooms.update(cx, |model, cx| {
                            model.extend(events);
                            cx.notify();
                        });
                    });
                }
            })
            .detach();
    }

    pub fn new_message(&mut self, event: Event, metadata: Option<Metadata>, cx: &mut AppContext) {
        // Get room id
        let room_id = SharedString::from(get_room_id(&event.pubkey, &event.tags));
        // Create message
        let message = Message::new(event, metadata);

        self.messages.update(cx, |this, cx| {
            this.write()
                .unwrap()
                .entry(room_id)
                .or_insert(Arc::new(RwLock::new(Vec::new())))
                .write()
                .unwrap()
                .push(message);

            cx.notify();
        });
    }

    pub fn messages(&self) -> WeakModel<Messages> {
        self.messages.downgrade()
    }

    pub fn rooms(&self) -> WeakModel<Vec<Event>> {
        self.rooms.downgrade()
    }
}
