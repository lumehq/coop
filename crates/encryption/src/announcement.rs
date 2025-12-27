use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Announcement {
    id: EventId,
    public_key: PublicKey,
    client_name: Option<String>,
}

impl Announcement {
    pub fn new(id: EventId, client_name: Option<String>, public_key: PublicKey) -> Self {
        Self {
            id,
            client_name,
            public_key,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn client_name(&self) -> SharedString {
        self.client_name
            .as_ref()
            .map(SharedString::from)
            .unwrap_or(SharedString::from("Unknown"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Response {
    payload: String,
    public_key: PublicKey,
}

impl Response {
    pub fn new(payload: String, public_key: PublicKey) -> Self {
        Self {
            payload,
            public_key,
        }
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn payload(&self) -> &str {
        self.payload.as_str()
    }
}
