use gpui::*;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use std::sync::{Arc, RwLock};

use crate::utils::get_room_id;

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Room {
    pub id: SharedString,
    pub owner: PublicKey,
    pub members: Vec<PublicKey>,
    pub last_seen: Timestamp,
    pub title: Option<SharedString>,
}

impl Room {
    pub fn new(event: &Event) -> Self {
        let owner = event.pubkey;
        let last_seen = event.created_at;
        // Get all members from event's tag
        let members: Vec<PublicKey> = event.tags.public_keys().copied().collect();
        // Get title from event's tag
        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            // TODO: create random name?
            None
        };

        // Get unique id based on members
        let id = get_room_id(&owner, &members).into();

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

    pub fn set_init(&mut self) {
        self.is_initialized = true;
    }

    pub fn set_reload(&mut self) {
        self.reload = true;
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
