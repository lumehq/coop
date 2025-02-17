use dirs::config_dir;
use nostr_sdk::prelude::*;
use std::{fs, sync::OnceLock, time::Duration};

static CLIENT: OnceLock<Client> = OnceLock::new();

pub fn initialize_client() -> &'static Client {
    // Setup app data folder
    let config_dir = config_dir().expect("Config directory not found");
    let app_dir = config_dir.join("Coop/");

    // Create app directory if it doesn't exist
    _ = fs::create_dir_all(&app_dir);

    // Setup database
    let lmdb = NostrLMDB::open(app_dir.join("nostr")).expect("Database is NOT initialized");

    // Client options
    let opts = Options::new()
        // NIP-65
        .gossip(true)
        // Skip all very slow relays
        .max_avg_latency(Duration::from_millis(800));

    // Setup Nostr Client
    let client = ClientBuilder::default().database(lmdb).opts(opts).build();

    CLIENT.set(client).expect("Client is already initialized!");
    CLIENT.get().expect("Client is NOT initialized!")
}

pub fn get_client() -> &'static Client {
    CLIENT.get().expect("Client is NOT initialized!")
}
