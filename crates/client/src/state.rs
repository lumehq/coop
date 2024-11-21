use dirs::config_dir;
use nostr_sdk::prelude::*;
use std::fs;
use tokio::sync::OnceCell;

pub static CLIENT: OnceCell<Client> = OnceCell::const_new();

pub async fn get_client() -> &'static Client {
    CLIENT
        .get_or_init(|| async {
            // Setup app data folder
            let config_dir = config_dir().unwrap();
            let _ = fs::create_dir_all(config_dir.join("Coop/"));

            // Setup database
            let lmdb = NostrLMDB::open(config_dir.join("Coop/nostr"))
                .expect("Database is NOT initialized");

            // Setup Nostr Client
            let client = ClientBuilder::default().database(lmdb).build();

            // Add some bootstrap relays
            let _ = client.add_relay("wss://relay.damus.io").await;
            let _ = client.add_relay("wss://relay.primal.net").await;
            let _ = client.add_relay("wss://nostr.fmt.wiz.biz").await;
            let _ = client.add_relay("wss://directory.yabu.me").await;
            let _ = client.add_relay("wss://purplepag.es").await;
            let _ = client.add_relay("wss://user.kindpag.es/").await;

            // Connect to all relays
            client.connect().await;

            // Return client
            client
        })
        .await
}
