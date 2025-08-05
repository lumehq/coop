use std::hash::Hash;
use std::iter::IntoIterator;

use chrono::{Local, TimeZone};
use gpui::SharedString;
use nostr_sdk::prelude::*;

use crate::room::SendError;

/// Represents a message in the chat system.
///
/// Contains information about the message content, author, creation time,
/// mentions, replies, and any errors that occurred during sending.
#[derive(Debug, Clone)]
pub struct Message {
    /// Unique identifier of the message (EventId from nostr_sdk)
    pub id: EventId,
    /// Author's public key
    pub author: PublicKey,
    /// The content/text of the message
    pub content: SharedString,
    /// When the message was created
    pub created_at: Timestamp,
    /// List of mentioned public keys in the message
    pub mentions: Vec<PublicKey>,
    /// List of EventIds this message is replying to
    pub replies_to: Option<Vec<EventId>>,
    /// Any errors that occurred while sending this message
    pub errors: Option<Vec<SendError>>,
}

impl Eq for Message {}

impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.created_at.cmp(&other.created_at)
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for Message {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Builder pattern implementation for constructing Message objects.
#[derive(Debug)]
pub struct MessageBuilder {
    id: EventId,
    author: PublicKey,
    content: Option<SharedString>,
    created_at: Option<Timestamp>,
    mentions: Vec<PublicKey>,
    replies_to: Option<Vec<EventId>>,
    errors: Option<Vec<SendError>>,
}

impl MessageBuilder {
    /// Creates a new MessageBuilder with default values
    pub fn new(id: EventId, author: PublicKey) -> Self {
        Self {
            id,
            author,
            content: None,
            created_at: None,
            mentions: vec![],
            replies_to: None,
            errors: None,
        }
    }

    /// Sets the message content
    pub fn content(mut self, content: impl Into<SharedString>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Sets the creation timestamp
    pub fn created_at(mut self, created_at: Timestamp) -> Self {
        self.created_at = Some(created_at);
        self
    }

    /// Adds a single mention to the message
    pub fn mention(mut self, mention: PublicKey) -> Self {
        self.mentions.push(mention);
        self
    }

    /// Adds multiple mentions to the message
    pub fn mentions<I>(mut self, mentions: I) -> Self
    where
        I: IntoIterator<Item = PublicKey>,
    {
        self.mentions.extend(mentions);
        self
    }

    /// Sets a single message this is replying to
    pub fn reply_to(mut self, reply_to: EventId) -> Self {
        self.replies_to = Some(vec![reply_to]);
        self
    }

    /// Sets multiple messages this is replying to
    pub fn replies_to<I>(mut self, replies_to: I) -> Self
    where
        I: IntoIterator<Item = EventId>,
    {
        let replies: Vec<EventId> = replies_to.into_iter().collect();
        if !replies.is_empty() {
            self.replies_to = Some(replies);
        }
        self
    }

    /// Adds errors that occurred during sending
    pub fn errors<I>(mut self, errors: I) -> Self
    where
        I: IntoIterator<Item = SendError>,
    {
        self.errors = Some(errors.into_iter().collect());
        self
    }

    /// Builds the message
    pub fn build(self) -> Result<Message, String> {
        Ok(Message {
            id: self.id,
            author: self.author,
            content: self.content.ok_or("Content is required")?,
            created_at: self.created_at.unwrap_or_else(Timestamp::now),
            mentions: self.mentions,
            replies_to: self.replies_to,
            errors: self.errors,
        })
    }
}

impl Message {
    /// Creates a new MessageBuilder
    pub fn builder(id: EventId, author: PublicKey) -> MessageBuilder {
        MessageBuilder::new(id, author)
    }

    /// Returns a human-readable string representing how long ago the message was created
    pub fn ago(&self) -> SharedString {
        let input_time = match Local.timestamp_opt(self.created_at.as_u64() as i64, 0) {
            chrono::LocalResult::Single(time) => time,
            _ => return "Invalid timestamp".into(),
        };

        let now = Local::now();
        let input_date = input_time.date_naive();
        let now_date = now.date_naive();
        let yesterday_date = (now - chrono::Duration::days(1)).date_naive();

        let time_format = input_time.format("%H:%M %p");

        match input_date {
            date if date == now_date => format!("Today at {time_format}"),
            date if date == yesterday_date => format!("Yesterday at {time_format}"),
            _ => format!("{}, {time_format}", input_time.format("%d/%m/%y")),
        }
        .into()
    }
}
