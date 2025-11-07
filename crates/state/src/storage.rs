use std::collections::{HashMap, HashSet};

use nostr_sdk::prelude::*;

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

#[derive(Debug, Clone, Default)]
pub struct CacheManager {
    /// Cache of messaging relays for each public key
    relay: HashMap<PublicKey, HashSet<RelayUrl>>,

    /// Cache of device announcement for each public key
    announcement: HashMap<PublicKey, Option<Announcement>>,
}

impl CacheManager {
    pub fn relay(&self, public_key: &PublicKey) -> Option<&HashSet<RelayUrl>> {
        self.relay.get(public_key)
    }

    pub fn insert_relay(&mut self, public_key: PublicKey, urls: Vec<RelayUrl>) {
        self.relay.entry(public_key).or_default().extend(urls);
    }

    pub fn announcement(&self, public_key: &PublicKey) -> Option<&Option<Announcement>> {
        self.announcement.get(public_key)
    }

    pub fn insert_announcement(
        &mut self,
        public_key: PublicKey,
        announcement: Option<Announcement>,
    ) {
        self.announcement.insert(public_key, announcement);
    }
}
