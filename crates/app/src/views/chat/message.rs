use common::{last_seen::LastSeen, profile::NostrProfile};
use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(PartialEq, Eq)]
pub struct ParsedMessage {
    pub avatar: SharedString,
    pub display_name: SharedString,
    pub created_at: SharedString,
    pub content: String,
}

impl ParsedMessage {
    pub fn new(profile: &NostrProfile, content: &str, created_at: Timestamp) -> Self {
        let content = content.to_owned();
        let created_at = LastSeen(created_at).human_readable();

        Self {
            avatar: profile.avatar.clone(),
            display_name: profile.name.clone(),
            created_at,
            content,
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum Message {
    User(Box<ParsedMessage>),
    System(SharedString),
    Placeholder,
}

impl Message {
    pub fn new(message: ParsedMessage) -> Self {
        Self::User(Box::new(message))
    }

    pub fn system(content: SharedString) -> Self {
        Self::System(content)
    }

    pub fn placeholder() -> Self {
        Self::Placeholder
    }
}
