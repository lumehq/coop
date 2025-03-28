use common::{last_seen::LastSeen, profile::NostrProfile};
use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: EventId,
    pub content: String,
    pub author: NostrProfile,
    pub mentions: Vec<NostrProfile>,
    pub created_at: LastSeen,
}

impl Message {
    pub fn new(
        id: EventId,
        content: String,
        author: NostrProfile,
        mentions: Vec<NostrProfile>,
        created_at: Timestamp,
    ) -> Self {
        let created_at = LastSeen(created_at);

        Self {
            id,
            content,
            author,
            mentions,
            created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomMessage {
    /// User message
    User(Box<Message>),
    /// System message
    System(SharedString),
    /// Only use for UI purposes.
    /// Placeholder will be used for display room announcement
    Announcement,
}

impl RoomMessage {
    pub fn user(message: Message) -> Self {
        Self::User(Box::new(message))
    }

    pub fn system(content: SharedString) -> Self {
        Self::System(content)
    }

    pub fn announcement() -> Self {
        Self::Announcement
    }
}
