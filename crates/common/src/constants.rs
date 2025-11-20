pub const CLIENT_NAME: &str = "Coop";
pub const APP_ID: &str = "su.reya.coop";

/// Bootstrap Relays.
pub const BOOTSTRAP_RELAYS: [&str; 5] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://relay.nos.social",
    "wss://user.kindpag.es",
    "wss://purplepag.es",
];

/// Search Relays.
pub const SEARCH_RELAYS: [&str; 3] = [
    "wss://relay.nostr.band",
    "wss://search.nos.today",
    "wss://relay.noswhere.com",
];

/// Default relay for Nostr Connect
pub const NOSTR_CONNECT_RELAY: &str = "wss://relay.nsec.app";

/// Default retry count for fetching NIP-17 relays
pub const RELAY_RETRY: u64 = 2;

/// Default retry count for sending messages
pub const SEND_RETRY: u64 = 10;

/// Default timeout (in seconds) for Nostr Connect
pub const NOSTR_CONNECT_TIMEOUT: u64 = 200;

/// Default timeout (in seconds) for Nostr Connect (Bunker)
pub const BUNKER_TIMEOUT: u64 = 30;

/// Total metadata requests will be grouped.
pub const METADATA_BATCH_LIMIT: usize = 20;

/// Default width of the sidebar.
pub const DEFAULT_SIDEBAR_WIDTH: f32 = 240.;
