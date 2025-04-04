use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::lock::Mutex;

use std::{collections::HashSet, sync::OnceLock, time::Duration};

pub mod constants;
pub mod paths;

static CLIENT: OnceLock<Client> = OnceLock::new();
static CLIENT_KEYS: OnceLock<Keys> = OnceLock::new();
static VERIFIED_PUBKEYS: OnceLock<Mutex<HashSet<PublicKey>>> = OnceLock::new();

/// Nostr Client instance
pub fn get_client() -> &'static Client {
    CLIENT.get_or_init(|| {
        // Setup database
        let db_path = nostr_file();
        let lmdb = NostrLMDB::open(db_path).expect("Database is NOT initialized");

        // Client options
        let opts = Options::new()
            // NIP-65
            .gossip(true)
            // Skip all very slow relays
            .max_avg_latency(Duration::from_secs(2));

        // Setup Nostr Client
        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}

/// Client Keys
pub fn get_client_keys() -> &'static Keys {
    CLIENT_KEYS.get_or_init(Keys::generate)
}

/// Verified Public Keys
pub fn get_verified_pubkeys() -> &'static Mutex<HashSet<PublicKey>> {
    VERIFIED_PUBKEYS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Add a verified public key
pub async fn add_verified_pubkey(pubkey: PublicKey) {
    let mut verified_pubkeys = get_verified_pubkeys().lock().await;
    verified_pubkeys.insert(pubkey);
}
