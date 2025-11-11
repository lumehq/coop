use std::hash::{DefaultHasher, Hash, Hasher};

use itertools::Itertools;
use nostr_sdk::prelude::*;

pub trait EventUtils {
    fn uniq_id(&self) -> u64;
    fn all_pubkeys(&self) -> Vec<PublicKey>;
}

impl EventUtils for Event {
    fn uniq_id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut pubkeys: Vec<PublicKey> = self.all_pubkeys();
        pubkeys.sort();
        pubkeys.hash(&mut hasher);
        hasher.finish()
    }

    fn all_pubkeys(&self) -> Vec<PublicKey> {
        let mut public_keys: Vec<PublicKey> = self.tags.public_keys().copied().collect();
        public_keys.push(self.pubkey);

        public_keys.into_iter().unique().collect()
    }
}

impl EventUtils for UnsignedEvent {
    fn uniq_id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut pubkeys: Vec<PublicKey> = vec![];

        // Add all public keys from event
        pubkeys.push(self.pubkey);
        pubkeys.extend(self.tags.public_keys().collect::<Vec<_>>());

        // Generate unique hash
        pubkeys
            .into_iter()
            .unique()
            .sorted()
            .collect::<Vec<_>>()
            .hash(&mut hasher);

        hasher.finish()
    }

    fn all_pubkeys(&self) -> Vec<PublicKey> {
        let mut public_keys: Vec<PublicKey> = self.tags.public_keys().copied().collect();
        public_keys.push(self.pubkey);
        public_keys.into_iter().unique().sorted().collect()
    }
}
