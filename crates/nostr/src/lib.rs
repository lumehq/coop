use gpui::Global;
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;
use state::get_client;
use std::collections::HashSet;

pub mod state;

pub struct Nostr {
    pub client: &'static Client,
    pub accounts: HashSet<PublicKey>,
}

// Set Nostr as Global State
impl Global for Nostr {}

impl Nostr {
    pub async fn init() -> Self {
        // Initialize nostr client
        let client = get_client().await;
        // Get all accounts
        let accounts = Self::get_accounts();

        Self { client, accounts }
    }

    pub fn get_accounts() -> HashSet<PublicKey> {
        let search = Search::new().expect("Keyring not working.");
        let results = search.by_service("coop");
        let list = List::list_credentials(&results, Limit::All);
        let accounts: HashSet<PublicKey> = list
            .split_whitespace()
            .filter(|v| v.starts_with("npub1") && !v.ends_with("coop"))
            .filter_map(|i| PublicKey::from_bech32(i).ok())
            .collect();

        accounts
    }
}
