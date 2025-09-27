use std::hash::Hash;

use nostr_sdk::prelude::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Message {
    User(RenderedMessage),
    Warning(String, Timestamp),
    System(Timestamp),
}

impl Message {
    pub fn user(user: impl Into<RenderedMessage>) -> Self {
        Self::User(user.into())
    }

    pub fn warning(content: String) -> Self {
        Self::Warning(content, Timestamp::now())
    }

    pub fn system() -> Self {
        Self::System(Timestamp::default())
    }

    fn timestamp(&self) -> &Timestamp {
        match self {
            Message::User(msg) => &msg.created_at,
            Message::Warning(_, ts) => ts,
            Message::System(ts) => ts,
        }
    }
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            // System always comes first
            (Message::System(_), Message::System(_)) => self.timestamp().cmp(other.timestamp()),
            (Message::System(_), _) => std::cmp::Ordering::Less,
            (_, Message::System(_)) => std::cmp::Ordering::Greater,

            // For non-system messages, compare by timestamp
            _ => self.timestamp().cmp(other.timestamp()),
        }
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct RenderedMessage {
    pub id: EventId,
    /// Author's public key
    pub author: PublicKey,
    /// The content/text of the message
    pub content: String,
    /// Message created time as unix timestamp
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
            content: inner.content,
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
            content: inner.content,
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
        if let Some(id) = tag.content().and_then(|id| EventId::parse(id).ok()) {
            replies_to.push(id);
        }
    }

    for tag in inner.filter(TagKind::q()) {
        if let Some(id) = tag.content().and_then(|id| EventId::parse(id).ok()) {
            replies_to.push(id);
        }
    }

    replies_to
}
