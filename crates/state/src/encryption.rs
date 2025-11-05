use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize, Serialize)]
pub enum SignerKind {
    Encryption,
    #[default]
    User,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Announcement {
    id: EventId,
    client: String,
    public_key: PublicKey,
}

impl Announcement {
    pub fn new(id: EventId, client_name: String, public_key: PublicKey) -> Self {
        Self {
            id,
            client: client_name,
            public_key,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn client(&self) -> &str {
        self.client.as_str()
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
