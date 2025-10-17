use std::sync::OnceLock;
use std::time::Duration;

use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use paths::nostr_file;

use crate::state::AppState;

pub mod constants;
pub mod paths;
pub mod state;

static APP_STATE: OnceLock<AppState> = OnceLock::new();
static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();
static NIP65_RELAYS: OnceLock<Vec<(RelayUrl, Option<RelayMetadata>)>> = OnceLock::new();
static NIP17_RELAYS: OnceLock<Vec<RelayUrl>> = OnceLock::new();

/// Initialize the application state.
pub fn app_state() -> &'static AppState {
    APP_STATE.get_or_init(AppState::new)
}

/// Initialize the nostr client.
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

/// Default NIP65 Relays. Used for new account
pub fn default_nip65_relays() -> &'static Vec<(RelayUrl, Option<RelayMetadata>)> {
    NIP65_RELAYS.get_or_init(|| {
        vec![
            (
                RelayUrl::parse("wss://nostr.mom").unwrap(),
                Some(RelayMetadata::Read),
            ),
            (
                RelayUrl::parse("wss://nostr.bitcoiner.social").unwrap(),
                Some(RelayMetadata::Read),
            ),
            (
                RelayUrl::parse("wss://nostr.oxtr.dev").unwrap(),
                Some(RelayMetadata::Write),
            ),
            (
                RelayUrl::parse("wss://nostr.fmt.wiz.biz").unwrap(),
                Some(RelayMetadata::Write),
            ),
            (RelayUrl::parse("wss://relay.primal.net").unwrap(), None),
            (RelayUrl::parse("wss://relay.damus.io").unwrap(), None),
        ]
    })
}

/// Default NIP17 Relays. Used for new account
pub fn default_nip17_relays() -> &'static Vec<RelayUrl> {
    NIP17_RELAYS.get_or_init(|| {
        vec![
            RelayUrl::parse("wss://nip17.com").unwrap(),
            RelayUrl::parse("wss://auth.nostr1.com").unwrap(),
        ]
    })
}
