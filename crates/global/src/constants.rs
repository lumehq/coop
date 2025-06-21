pub const APP_NAME: &str = "Coop";
pub const APP_ID: &str = "su.reya.coop";
pub const APP_PUBKEY: &str = "b1813fb01274b32cc5db6d1198e7c79dda0fb430899f63c7064f651a41d44f2b";
pub const KEYRING_URL: &str = "Coop Safe Storage";

pub const ACCOUNT_D: &str = "coop:account";
pub const SETTINGS_D: &str = "coop:settings";

/// Bootstrap Relays.
pub const BOOTSTRAP_RELAYS: [&str; 4] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://user.kindpag.es",
    "wss://relaydiscovery.com",
];

/// NIP65 Relays. Used for new account
pub const NIP65_RELAYS: [&str; 4] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://relay.nostr.net",
    "wss://nos.lol",
];

/// Messaging Relays. Used for new account
pub const NIP17_RELAYS: [&str; 2] = ["wss://auth.nostr1.com", "wss://relay.0xchat.com"];

/// Search Relays.
pub const SEARCH_RELAYS: [&str; 1] = ["wss://relay.nostr.band"];

/// Default relay for Nostr Connect
pub const NOSTR_CONNECT_RELAY: &str = "wss://relay.nsec.app";

/// Default timeout for Nostr Connect
pub const NOSTR_CONNECT_TIMEOUT: u64 = 300;

/// Unique ID for new message subscription.
pub const NEW_MESSAGE_SUB_ID: &str = "listen_new_giftwraps";
/// Unique ID for all messages subscription.
pub const ALL_MESSAGES_SUB_ID: &str = "listen_all_giftwraps";

/// Total metadata requests will be grouped.
pub const METADATA_BATCH_LIMIT: usize = 100;
/// Maximum timeout for grouping metadata requests.
pub const METADATA_BATCH_TIMEOUT: u64 = 400;

/// Default width for all modals.
pub const DEFAULT_MODAL_WIDTH: f32 = 420.;
/// Default width of the sidebar.
pub const DEFAULT_SIDEBAR_WIDTH: f32 = 280.;

/// Image Resize Service
pub const IMAGE_RESIZE_SERVICE: &str = "https://wsrv.nl";

/// Default NIP96 Media Server.
pub const NIP96_SERVER: &str = "https://nostrmedia.com";

pub(crate) const GLOBAL_CHANNEL_LIMIT: usize = 2048;
pub(crate) const BATCH_CHANNEL_LIMIT: usize = 2048;
