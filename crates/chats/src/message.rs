use std::cell::RefCell;
use std::iter::IntoIterator;
use std::rc::Rc;

use chrono::{Local, TimeZone};
use gpui::SharedString;
use nostr_sdk::prelude::*;

use crate::room::SendError;

/// Represents a message in the chat system.
///
/// Contains information about the message content, author, creation time,
/// mentions, replies, and any errors that occurred during sending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Unique identifier of the message (EventId from nostr_sdk)
    pub id: Option<EventId>,
    /// Author profile information
    pub author: Option<Profile>,
    /// The content/text of the message
    pub content: SharedString,
    /// When the message was created
    pub created_at: Timestamp,
    /// List of mentioned profiles in the message
    pub mentions: Vec<Profile>,
    /// List of EventIds this message is replying to
    pub replies_to: Option<Vec<EventId>>,
    /// Any errors that occurred while sending this message
    pub errors: Option<Vec<SendError>>,
}

/// Builder pattern implementation for constructing Message objects.
#[derive(Debug, Default)]
pub struct MessageBuilder {
    id: Option<EventId>,
    author: Option<Profile>,
    content: Option<String>,
    created_at: Option<Timestamp>,
    mentions: Vec<Profile>,
    replies_to: Option<Vec<EventId>>,
    errors: Option<Vec<SendError>>,
}

impl MessageBuilder {
    /// Creates a new MessageBuilder with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the message ID
    pub fn id(mut self, id: EventId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the message author
    pub fn author(mut self, author: Profile) -> Self {
        self.author = Some(author);
        self
    }

    /// Sets the message content
    pub fn content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }

    /// Sets the creation timestamp
    pub fn created_at(mut self, created_at: Timestamp) -> Self {
        self.created_at = Some(created_at);
        self
    }

    /// Adds a single mention to the message
    pub fn mention(mut self, mention: Profile) -> Self {
        self.mentions.push(mention);
        self
    }

    /// Adds multiple mentions to the message
    pub fn mentions<I>(mut self, mentions: I) -> Self
    where
        I: IntoIterator<Item = Profile>,
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

    /// Builds the message wrapped in an Rc<RefCell<Message>>
    pub fn build_rc(self) -> Result<Rc<RefCell<Message>>, String> {
        self.build().map(|m| Rc::new(RefCell::new(m)))
    }

    /// Builds the message
    pub fn build(self) -> Result<Message, String> {
        Ok(Message {
            id: self.id,
            author: self.author,
            content: self.content.ok_or("Content is required")?.into(),
            created_at: self.created_at.unwrap_or_else(Timestamp::now),
            mentions: self.mentions,
            replies_to: self.replies_to,
            errors: self.errors,
        })
    }
}

impl Message {
    /// Creates a new MessageBuilder
    pub fn builder() -> MessageBuilder {
        MessageBuilder::new()
    }

    /// Converts the message into an Rc<RefCell<Message>>
    pub fn into_rc(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }

    /// Builds a message from a builder and wraps it in Rc<RefCell>
    pub fn build_rc(builder: MessageBuilder) -> Result<Rc<RefCell<Self>>, String> {
        builder.build().map(|m| Rc::new(RefCell::new(m)))
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
