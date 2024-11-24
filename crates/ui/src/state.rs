use gpui::Global;
use nostr_sdk::prelude::*;

pub struct AppState {
    pub signer: Option<PublicKey>,
}

impl Global for AppState {}

impl AppState {
    pub fn new() -> Self {
        Self { signer: None }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
