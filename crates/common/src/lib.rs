use std::sync::OnceLock;

pub use constants::*;
pub use debounced_delay::*;
pub use display::*;
pub use event::*;
pub use nip05::*;
pub use nip96::*;
use nostr_sdk::prelude::*;
pub use paths::*;

mod constants;
mod debounced_delay;
mod display;
mod event;
mod nip05;
mod nip96;
mod paths;

static APP_NAME: OnceLock<String> = OnceLock::new();
static NIP65_RELAYS: OnceLock<Vec<(RelayUrl, Option<RelayMetadata>)>> = OnceLock::new();
static NIP17_RELAYS: OnceLock<Vec<RelayUrl>> = OnceLock::new();

/// Get the app name
pub fn app_name() -> &'static String {
    APP_NAME.get_or_init(|| {
        let devicename = whoami::devicename();
        let platform = whoami::platform();

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
                RelayUrl::parse("wss://nos.lol").unwrap(),
                Some(RelayMetadata::Write),
            ),
            (
                RelayUrl::parse("wss://relay.snort.social").unwrap(),
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
