use std::collections::{HashMap, HashSet};

use nostr_sdk::prelude::*;

use crate::Announcement;

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
