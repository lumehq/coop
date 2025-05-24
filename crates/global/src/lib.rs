use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::lock::RwLock;

use std::{collections::BTreeMap, sync::OnceLock, time::Duration};

pub mod constants;
pub mod paths;

/// Represents the global state of the Nostr client, including:
/// - The Nostr client instance
/// - Client keys
/// - A cache of user profiles (metadata)
pub struct NostrState {
    keys: Keys,
    client: Client,
    cache_profiles: RwLock<BTreeMap<PublicKey, Option<Metadata>>>,
}

/// Global singleton instance of NostrState
static GLOBAL_STATE: OnceLock<NostrState> = OnceLock::new();

/// Initializes and returns a new NostrState instance with:
/// - LMDB database backend
/// - Default client options (gossip enabled, 800ms max avg latency)
/// - Newly generated keys
/// - Empty profile cache
pub fn init_global_state() -> NostrState {
    // rustls uses the `aws_lc_rs` provider by default
    // This only errors if the default provider has already
    // been installed. We can ignore this `Result`.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();

    // Setup database
    let db_path = nostr_file();
    let lmdb = NostrLMDB::open(db_path).expect("Database is NOT initialized");

    // Client options
    let opts = Options::new()
        .gossip(true)
        .max_avg_latency(Duration::from_millis(800));

    NostrState {
        client: ClientBuilder::default().database(lmdb).opts(opts).build(),
        keys: Keys::generate(),
        cache_profiles: RwLock::new(BTreeMap::new()),
    }
}

/// Returns a reference to the global Nostr client instance.
///
/// Initializes the global state if it hasn't been initialized yet.
pub fn get_client() -> &'static Client {
    &GLOBAL_STATE.get_or_init(init_global_state).client
}

/// Returns a reference to the client's cryptographic keys.
///
/// Initializes the global state if it hasn't been initialized yet.
pub fn get_client_keys() -> &'static Keys {
    &GLOBAL_STATE.get_or_init(init_global_state).keys
}

/// Returns a reference to the global profile cache (thread-safe).
///
/// Initializes the global state if it hasn't been initialized yet.
pub fn profiles() -> &'static RwLock<BTreeMap<PublicKey, Option<Metadata>>> {
    &GLOBAL_STATE.get_or_init(init_global_state).cache_profiles
}

/// Synchronously gets a profile from the cache by public key.
///
/// Returns default metadata if the profile is not cached.
pub fn get_cache_profile(key: &PublicKey) -> Profile {
    let metadata = if let Some(metadata) = profiles().read_blocking().get(key) {
        metadata.clone().unwrap_or_default()
    } else {
        Metadata::default()
    };

    Profile::new(*key, metadata)
}

/// Asynchronously gets a profile from the cache by public key.
///
/// Returns default metadata if the profile isn't cached.
pub async fn async_cache_profile(key: &PublicKey) -> Profile {
    let metadata = if let Some(metadata) = profiles().read().await.get(key) {
        metadata.clone().unwrap_or_default()
    } else {
        Metadata::default()
    };

    Profile::new(*key, metadata)
}

/// Synchronously inserts or updates a profile in the cache.
pub fn insert_cache_profile(key: PublicKey, metadata: Option<Metadata>) {
    profiles()
        .write_blocking()
        .entry(key)
        .and_modify(|entry| {
            if entry.is_none() {
                *entry = metadata.clone();
            }
        })
        .or_insert_with(|| metadata);
}
