use chrono::{Local, TimeZone};
use gpui::SharedString;
use nostr_sdk::prelude::*;
use std::{cell::RefCell, iter::IntoIterator, rc::Rc};

use crate::room::SendError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Option<EventId>,
    pub author: Option<Profile>,
    pub content: SharedString,
    pub created_at: Timestamp,
    pub mentions: Vec<Profile>,
    pub replies_to: Option<Vec<EventId>>,
    pub errors: Option<Vec<SendError>>,
}

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: EventId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn author(mut self, author: Profile) -> Self {
        self.author = Some(author);
        self
    }

    pub fn content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }

    pub fn created_at(mut self, created_at: Timestamp) -> Self {
        self.created_at = Some(created_at);
        self
    }

    pub fn mention(mut self, mention: Profile) -> Self {
        self.mentions.push(mention);
        self
    }

    pub fn mentions<I>(mut self, mentions: I) -> Self
    where
        I: IntoIterator<Item = Profile>,
    {
        self.mentions.extend(mentions);
        self
    }

    pub fn reply_to(mut self, reply_to: EventId) -> Self {
        self.replies_to = Some(vec![reply_to]);
        self
    }

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

    pub fn errors<I>(mut self, errors: I) -> Self
    where
        I: IntoIterator<Item = SendError>,
    {
        self.errors = Some(errors.into_iter().collect());
        self
    }

    pub fn build_rc(self) -> Result<Rc<RefCell<Message>>, String> {
        self.build().map(|m| Rc::new(RefCell::new(m)))
    }

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
    pub fn builder() -> MessageBuilder {
        MessageBuilder::new()
    }

    pub fn into_rc(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }

    pub fn build_rc(builder: MessageBuilder) -> Result<Rc<RefCell<Self>>, String> {
        builder.build().map(|m| Rc::new(RefCell::new(m)))
    }

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
