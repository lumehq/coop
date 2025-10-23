pub const APP_NAME: &str = "Coop";
pub const APP_ID: &str = "su.reya.coop";
pub const APP_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc4MkNFRkQ2RkVGQURGNzUKUldSMTMvcisxdThzZUZraHc4Vno3NVNJek81VkJFUEV3MkJweGFxQXhpekdSU1JIekpqMG4yemMK";
pub const APP_UPDATER_ENDPOINT: &str = "https://coop-updater.reya.su/";

pub const SETTINGS_IDENTIFIER: &str = "coop:settings";

/// Bootstrap Relays.
pub const BOOTSTRAP_RELAYS: [&str; 5] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://relay.nos.social",
    "wss://user.kindpag.es",
    "wss://purplepag.es",
];

/// Search Relays.
pub const SEARCH_RELAYS: [&str; 1] = ["wss://relay.nostr.band"];

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

/// Default timeout (in seconds) for fetching events
pub const QUERY_TIMEOUT: u64 = 5;

/// Total metadata requests will be grouped.
pub const METADATA_BATCH_LIMIT: usize = 100;

/// Maximum timeout for grouping metadata requests. (milliseconds)
pub const METADATA_BATCH_TIMEOUT: u64 = 300;

/// Default width of the sidebar.
pub const DEFAULT_SIDEBAR_WIDTH: f32 = 240.;

/// Image Resize Service
pub const IMAGE_RESIZE_SERVICE: &str = "https://wsrv.nl";

/// Default NIP96 Media Server.
pub const NIP96_SERVER: &str = "https://nostrmedia.com";
