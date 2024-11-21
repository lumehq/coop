use gpui::Global;
use nostr_sdk::prelude::*;
use std::collections::HashSet;

use crate::utils::get_all_accounts_from_keyring;

pub struct AppState {
    pub accounts: HashSet<PublicKey>,
}

impl Global for AppState {}

impl AppState {
    pub fn new() -> Self {
        let accounts = get_all_accounts_from_keyring();
        Self { accounts }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
