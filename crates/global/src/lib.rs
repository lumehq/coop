use nostr_sdk::prelude::*;
use paths::nostr_file;

use std::{sync::OnceLock, time::Duration};

pub mod constants;
pub mod paths;

static CLIENT: OnceLock<Client> = OnceLock::new();
static CLIENT_KEYS: OnceLock<Keys> = OnceLock::new();

/// Nostr Client instance
pub fn get_client() -> &'static Client {
    CLIENT.get_or_init(|| {
        // Setup database
        let db_path = nostr_file();
        let lmdb = NostrLMDB::open(db_path).expect("Database is NOT initialized");

        // Client options
        let opts = Options::new()
            // NIP-65
            // Coop is don't really need to enable this option,
            // but this will help the client discover user's messaging relays efficiently.
            .gossip(true)
            // Skip all very slow relays
            // Note: max delay is 800ms
            .max_avg_latency(Duration::from_millis(800));

        // Setup Nostr Client
        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}

/// Client Keys
pub fn get_client_keys() -> &'static Keys {
    CLIENT_KEYS.get_or_init(Keys::generate)
}
