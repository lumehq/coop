use crate::{get_client, utils::room_hash};
use gpui::{AppContext, Context, Global, Model, SharedString, WeakModel};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use profile::cut_public_key;
use rnglib::{Language, RNG};
use serde::Deserialize;
use std::{
    cmp::Reverse,
    collections::HashMap,
    sync::{Arc, RwLock},
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Member {
    public_key: PublicKey,
    metadata: Metadata,
}

impl Member {
    pub fn new(public_key: PublicKey, metadata: Metadata) -> Self {
        Self {
            public_key,
            metadata,
        }
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    pub fn name(&self) -> String {
        if let Some(display_name) = &self.metadata.display_name {
            if !display_name.is_empty() {
                return display_name.clone();
            }
        }

        if let Some(name) = &self.metadata.name {
            if !name.is_empty() {
                return name.clone();
            }
        }

        cut_public_key(self.public_key)
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Room {
    pub id: SharedString,
    pub owner: PublicKey,
    pub members: Vec<Member>,
    pub last_seen: Timestamp,
    pub title: Option<SharedString>,
}

impl Room {
    pub fn new(
        id: SharedString,
        owner: PublicKey,
        last_seen: Timestamp,
        title: Option<SharedString>,
        members: Vec<Member>,
    ) -> Self {
        let title = if title.is_none() {
            let rng = RNG::from(&Language::Roman);
            let name = rng.generate_names(2, true).join("-").to_lowercase();

            Some(name.into())
        } else {
            title
        };

        Self {
            id,
            title,
            members,
            last_seen,
            owner,
        }
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

type Inbox = Vec<Event>;
type Messages = RwLock<HashMap<SharedString, Arc<RwLock<Vec<Message>>>>>;

pub struct ChatRegistry {
    messages: Model<Messages>,
    inbox: Model<Inbox>,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let inbox = cx.new_model(|_| Vec::new());
        let messages = cx.new_model(|_| RwLock::new(HashMap::new()));

        cx.set_global(Self { inbox, messages });
    }

    pub fn init(&mut self, cx: &mut AppContext) {
        let async_cx = cx.to_async();
        // Get all current room's hashes
        let hashes: Vec<u64> = self
            .inbox
            .read(cx)
            .iter()
            .map(|ev| room_hash(&ev.tags))
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
                        // 1. Only new rooms
                        // 2. Only unique rooms
                        // 3. Sorted by created_at
                        let result = events
                            .into_iter()
                            .filter(|ev| !hashes.iter().any(|h| h == &room_hash(&ev.tags)))
                            .unique_by(|ev| room_hash(&ev.tags))
                            .sorted_by_key(|ev| Reverse(ev.created_at))
                            .collect::<Vec<_>>();

                        Ok(result)
                    })
                    .await;

                if let Ok(events) = query {
                    _ = async_cx.update_global::<Self, _>(|state, cx| {
                        state.inbox.update(cx, |model, cx| {
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
        let room_id = SharedString::from(room_hash(&event.tags).to_string());
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

    pub fn inbox(&self) -> WeakModel<Inbox> {
        self.inbox.downgrade()
    }
}
