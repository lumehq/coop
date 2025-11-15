use std::collections::{HashMap, HashSet};

use gpui::SharedString;
use nostr_sdk::prelude::*;

use crate::NostrRegistry;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    pub fn client(&self) -> SharedString {
        SharedString::from(self.client.clone())
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
pub struct Gossip {
    /// Gossip relays for each public key
    relays: HashMap<PublicKey, HashSet<(RelayUrl, Option<RelayMetadata>)>>,

    /// Messaging relays for each public key
    messaging_relays: HashMap<PublicKey, HashSet<RelayUrl>>,

    /// Encryption announcement for each public key
    announcements: HashMap<PublicKey, Option<Announcement>>,
}

impl Gossip {
    /// Get inbox relays for a public key
    pub fn inbox_relays(&self, public_key: &PublicKey) -> Vec<RelayUrl> {
        self.relays
            .get(public_key)
            .map(|relays| {
                relays
                    .iter()
                    .filter_map(|(url, metadata)| {
                        if metadata.is_none() || metadata == &Some(RelayMetadata::Write) {
                            Some(url.to_owned())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get outbox relays for a public key
    pub fn outbox_relays(&self, public_key: &PublicKey) -> Vec<RelayUrl> {
        self.relays
            .get(public_key)
            .map(|relays| {
                relays
                    .iter()
                    .filter_map(|(url, metadata)| {
                        if metadata.is_none() || metadata == &Some(RelayMetadata::Read) {
                            Some(url.to_owned())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Insert gossip relays for a public key
    pub fn insert_relays(&mut self, event: &Event) {
        self.relays.entry(event.pubkey).or_default().extend(
            event
                .tags
                .iter()
                .filter_map(|tag| {
                    if let Some(TagStandard::RelayMetadata {
                        relay_url,
                        metadata,
                    }) = tag.clone().to_standardized()
                    {
                        Some((relay_url, metadata))
                    } else {
                        None
                    }
                })
                .take(3),
        );
    }

    /// Get messaging relays for a public key
    pub fn messaging_relays(&self, public_key: &PublicKey) -> Vec<RelayUrl> {
        self.messaging_relays
            .get(public_key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Insert messaging relays for a public key
    pub fn insert_messaging_relays(&mut self, event: &Event) {
        self.messaging_relays
            .entry(event.pubkey)
            .or_default()
            .extend(
                event
                    .tags
                    .iter()
                    .filter_map(|tag| {
                        if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
                            Some(url.to_owned())
                        } else {
                            None
                        }
                    })
                    .take(3),
            );
    }

    /// Ensure connections for the given relay list
    pub async fn ensure_connections(&self, client: &Client, urls: &[RelayUrl]) {
        for url in urls {
            client.add_relay(url).await.ok();
            client.connect_relay(url).await.ok();
        }
    }

    /// Get announcement for a public key
    pub fn announcement(&self, public_key: &PublicKey) -> Option<Announcement> {
        self.announcements
            .get(public_key)
            .cloned()
            .unwrap_or_default()
    }

    /// Insert announcement for a public key
    pub fn insert_announcement(&mut self, event: &Event) {
        let announcement = NostrRegistry::extract_announcement(event).ok();

        self.announcements
            .entry(event.pubkey)
            .or_insert(announcement);
    }
}
