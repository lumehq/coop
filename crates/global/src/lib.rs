use constants::{ALL_MESSAGES_SUB_ID, APP_ID};
use dirs::config_dir;
use nostr_sdk::prelude::*;
use smol::lock::Mutex;

use std::{
    fs,
    sync::{Arc, OnceLock},
    time::Duration,
};

pub mod constants;

static CLIENT: OnceLock<Client> = OnceLock::new();
static DEVICE_KEYS: Mutex<Option<Arc<dyn NostrSigner>>> = Mutex::new(None);
static APP_NAME: OnceLock<Arc<str>> = OnceLock::new();

/// Nostr Client instance
pub fn get_client() -> &'static Client {
    CLIENT.get_or_init(|| {
        // Setup app data folder
        let config_dir = config_dir().expect("Config directory not found");
        let app_dir = config_dir.join(APP_ID);

        // Create app directory if it doesn't exist
        _ = fs::create_dir_all(&app_dir);

        // Setup database
        let lmdb = NostrLMDB::open(app_dir.join("nostr")).expect("Database is NOT initialized");

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

/// Get app name
pub fn get_app_name() -> &'static str {
    APP_NAME.get_or_init(|| Arc::from(format!("Coop on {}", whoami::distro())))
}

/// Get device keys
pub async fn get_device_keys() -> Option<Arc<dyn NostrSigner>> {
    let guard = DEVICE_KEYS.lock().await;
    guard.clone()
}

/// Set device keys
pub async fn set_device_keys<T>(signer: T)
where
    T: NostrSigner + 'static,
{
    DEVICE_KEYS.lock().await.replace(Arc::new(signer));

    // Re-subscribe to all messages
    smol::spawn(async move {
        let client = get_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        if let Ok(signer) = client.signer().await {
            let public_key = signer.get_public_key().await.unwrap();

            // Create a filter for getting all gift wrapped events send to current user
            let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

            let id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            _ = client.unsubscribe(&id);
            _ = client.subscribe_with_id(id, filter, Some(opts)).await;
        }
    })
    .await;
}
