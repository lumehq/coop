use std::collections::{HashMap, HashSet};

use nostr_sdk::prelude::*;

/// Gossip
#[derive(Debug, Clone, Default)]
pub struct Gossip {
    /// Gossip relays for each public key
    relays: HashMap<PublicKey, HashSet<(RelayUrl, Option<RelayMetadata>)>>,
    /// Messaging relays for each public key
    messaging_relays: HashMap<PublicKey, HashSet<RelayUrl>>,
}

impl Gossip {
    /// Get read relays for a given public key
    pub fn read_relays(&self, public_key: &PublicKey) -> Vec<RelayUrl> {
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

    /// Get write relays for a given public key
    pub fn write_relays(&self, public_key: &PublicKey) -> Vec<RelayUrl> {
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

        log::info!("Updating gossip relays for: {}", event.pubkey);
    }

    /// Get messaging relays for a given public key
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

        log::info!("Updating messaging relays for: {}", event.pubkey);
    }
}
