use gpui::*;
use nostr_sdk::prelude::*;
use rnglib::{Language, RNG};
use serde::Deserialize;
use std::sync::{Arc, RwLock};

use super::metadata::MetadataRegistry;
use crate::utils::get_room_id;

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Room {
    pub id: SharedString,
    pub owner: PublicKey,
    pub members: Vec<PublicKey>,
    pub last_seen: Timestamp,
    pub title: Option<SharedString>,
    pub metadata: Option<Metadata>,
    is_initialized: bool,
}

impl Room {
    pub fn new(event: &Event, cx: &mut WindowContext<'_>) -> Self {
        let owner = event.pubkey;
        let last_seen = event.created_at;

        // Get all members from event's tag
        let members: Vec<PublicKey> = event.tags.public_keys().copied().collect();

        // Get title from event's tag
        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            let rng = RNG::from(&Language::Roman);
            let name = rng.generate_names(2, true).join("-").to_lowercase();

            Some(name.into())
        };

        // Get unique id based on members
        let id = get_room_id(&owner, &members).into();

        // Get metadata for all members if exists
        let metadata = cx.global::<MetadataRegistry>().get(&owner);

        Self {
            id,
            title,
            members,
            last_seen,
            owner,
            metadata,
            is_initialized: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub event: Event,
    pub metadata: Option<Metadata>,
}

pub struct ChatRegistry {
    pub new_messages: Arc<RwLock<Vec<Message>>>,
    pub reload: bool,
    pub is_initialized: bool,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());
    }

    pub fn update(&mut self) {
        if !self.is_initialized {
            self.is_initialized = true;
        } else {
            self.reload = true;
        }
    }

    pub fn push(&mut self, event: Event, metadata: Option<Metadata>) {
        self.new_messages
            .write()
            .unwrap()
            .push(Message { event, metadata });
    }

    fn new() -> Self {
        Self {
            new_messages: Arc::new(RwLock::new(Vec::new())),
            reload: false,
            is_initialized: false,
        }
    }
}
