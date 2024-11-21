use gpui::Global;
use keyring::Entry;
use nostr_sdk::prelude::*;
use state::get_client;

pub mod state;

pub struct NostrClient {
    pub client: &'static Client,
}

impl Global for NostrClient {}

impl NostrClient {
    pub async fn init() -> Self {
        // Initialize nostr client
        let client = get_client().await;

        Self { client }
    }

    pub fn add_account(&self, keys: Keys) -> Result<()> {
        let public_key = keys.public_key().to_bech32()?;
        let secret = keys.secret_key().to_secret_hex();
        let entry = Entry::new("Coop Safe Storage", &public_key)?;

        // Add secret to keyring
        entry.set_password(&secret)?;

        Ok(())
    }
}
