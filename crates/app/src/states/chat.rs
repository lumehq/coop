use gpui::*;
use nostr_sdk::prelude::*;
use serde::Deserialize;

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Room {
    pub owner: PublicKey,
    pub members: Vec<PublicKey>,
    pub last_seen: Timestamp,
    pub title: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub event: Event,
    pub metadata: Option<Metadata>,
}

pub struct ChatRegistry {
    pub new_messages: Vec<Message>,
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
        self.new_messages.push(Message { event, metadata });
    }

    fn new() -> Self {
        Self {
            new_messages: Vec::new(),
            reload: false,
            is_initialized: false,
        }
    }
}
