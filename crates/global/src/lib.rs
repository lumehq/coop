use std::sync::OnceLock;
use std::time::Duration;

use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use paths::nostr_file;

use crate::app_state::AppState;

pub mod app_state;
pub mod constants;
pub mod paths;

static APP_STATE: OnceLock<AppState> = OnceLock::new();
static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn app_state() -> &'static AppState {
    APP_STATE.get_or_init(AppState::new)
}

pub fn nostr_client() -> &'static Client {
    NOSTR_CLIENT.get_or_init(|| {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let lmdb = NostrLMDB::open(nostr_file()).expect("Database is NOT initialized");

        let opts = ClientOptions::new()
            .gossip(true)
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(600),
            });

        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}
