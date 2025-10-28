use std::sync::OnceLock;

use nostr_sdk::prelude::*;
use whoami::{devicename, platform};

mod constants;
mod paths;
mod state;

pub use constants::*;
pub use paths::*;
pub use state::*;

static APP_STATE: OnceLock<AppState> = OnceLock::new();
static APP_NAME: OnceLock<String> = OnceLock::new();
static NIP65_RELAYS: OnceLock<Vec<(RelayUrl, Option<RelayMetadata>)>> = OnceLock::new();
static NIP17_RELAYS: OnceLock<Vec<RelayUrl>> = OnceLock::new();

/// Initialize the application state.
pub fn app_state() -> &'static AppState {
    APP_STATE.get_or_init(AppState::new)
}

pub fn app_name() -> &'static String {
    APP_NAME.get_or_init(|| {
        let devicename = devicename();
        let platform = platform();

        format!("{CLIENT_NAME} on {platform} ({devicename})")
    })
}

/// Default NIP-65 Relays. Used for new account
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

/// Default NIP-17 Relays. Used for new account
pub fn default_nip17_relays() -> &'static Vec<RelayUrl> {
    NIP17_RELAYS.get_or_init(|| {
        vec![
            RelayUrl::parse("wss://nip17.com").unwrap(),
            RelayUrl::parse("wss://auth.nostr1.com").unwrap(),
        ]
    })
}
