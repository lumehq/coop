use std::hash::Hash;

use chrono::{Local, TimeZone};
use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone)]
pub struct SendReport {
    pub receiver: PublicKey,
    pub success: Option<Output<EventId>>,
    pub error: Option<String>,
}

impl SendReport {
    pub fn new(
        receiver: PublicKey,
        success: Option<Output<EventId>>,
        error: Option<String>,
    ) -> Self {
        Self {
            receiver,
            success,
            error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderedMessage {
    pub id: EventId,
    /// Author's public key
    pub author: PublicKey,
    /// The content/text of the message
    pub content: SharedString,
    /// When the message was created
    pub created_at: Timestamp,
    /// List of mentioned public keys in the message
    pub mentions: Vec<PublicKey>,
    /// List of event of the message this message is a reply to
    pub replies_to: Vec<EventId>,
}

impl From<Event> for RenderedMessage {
    fn from(inner: Event) -> Self {
        let mentions = extract_mentions(&inner.content);
        let replies_to = extract_reply_ids(&inner.tags);

        Self {
            id: inner.id,
            author: inner.pubkey,
            content: inner.content.into(),
            created_at: inner.created_at,
            mentions,
            replies_to,
        }
    }
}

impl From<UnsignedEvent> for RenderedMessage {
    fn from(inner: UnsignedEvent) -> Self {
        let mentions = extract_mentions(&inner.content);
        let replies_to = extract_reply_ids(&inner.tags);

        Self {
            // Event ID must be known
            id: inner.id.unwrap(),
            author: inner.pubkey,
            content: inner.content.into(),
            created_at: inner.created_at,
            mentions,
            replies_to,
        }
    }
}

impl From<Box<Event>> for RenderedMessage {
    fn from(inner: Box<Event>) -> Self {
        (*inner).into()
    }
}

impl From<&Box<Event>> for RenderedMessage {
    fn from(inner: &Box<Event>) -> Self {
        inner.to_owned().into()
    }
}

impl Eq for RenderedMessage {}

impl PartialEq for RenderedMessage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for RenderedMessage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.created_at.cmp(&other.created_at)
    }
}

impl PartialOrd for RenderedMessage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for RenderedMessage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl RenderedMessage {
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

fn extract_mentions(content: &str) -> Vec<PublicKey> {
    let parser = NostrParser::new();
    let tokens = parser.parse(content);

    tokens
        .filter_map(|token| match token {
            Token::Nostr(nip21) => match nip21 {
                Nip21::Pubkey(pubkey) => Some(pubkey),
                Nip21::Profile(profile) => Some(profile.public_key),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
}

fn extract_reply_ids(inner: &Tags) -> Vec<EventId> {
    let mut replies_to = vec![];

    for tag in inner.filter(TagKind::e()) {
        if let Some(content) = tag.content() {
            if let Ok(id) = EventId::from_hex(content) {
                replies_to.push(id);
            }
        }
    }

    for tag in inner.filter(TagKind::q()) {
        if let Some(content) = tag.content() {
            if let Ok(id) = EventId::from_hex(content) {
                replies_to.push(id);
            }
        }
    }

    replies_to
}
