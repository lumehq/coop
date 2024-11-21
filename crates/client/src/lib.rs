use gpui::Global;
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
}
