pub const APP_NAME: &str = "Coop";
pub const APP_ID: &str = "su.reya.coop";
pub const APP_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc4MkNFRkQ2RkVGQURGNzUKUldSMTMvcisxdThzZUZraHc4Vno3NVNJek81VkJFUEV3MkJweGFxQXhpekdSU1JIekpqMG4yemMK";
pub const APP_UPDATER_ENDPOINT: &str = "https://coop-updater.reya.su/";
pub const KEYRING_URL: &str = "Coop Safe Storage";

pub const ACCOUNT_D: &str = "coop:account";
pub const SETTINGS_D: &str = "coop:settings";

/// Bootstrap Relays.
pub const BOOTSTRAP_RELAYS: [&str; 4] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://user.kindpag.es",
    "wss://purplepag.es",
];

/// Search Relays.
pub const SEARCH_RELAYS: [&str; 2] = ["wss://search.nos.today", "wss://relay.nostr.band"];

/// NIP65 Relays. Used for new account
pub const NIP65_RELAYS: [&str; 4] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://relay.nostr.net",
    "wss://nos.lol",
];

/// Messaging Relays. Used for new account
pub const NIP17_RELAYS: [&str; 2] = ["wss://nip17.com", "wss://relay.0xchat.com"];

/// Default relay for Nostr Connect
pub const NOSTR_CONNECT_RELAY: &str = "wss://relay.nsec.app";

/// Default timeout (in seconds) for Nostr Connect
pub const NOSTR_CONNECT_TIMEOUT: u64 = 200;

/// Total metadata requests will be grouped.
pub const METADATA_BATCH_LIMIT: usize = 100;

/// Maximum timeout for grouping metadata requests. (milliseconds)
pub const METADATA_BATCH_TIMEOUT: u64 = 300;

/// Maximum timeout for waiting for finish (seconds)
pub const WAIT_FOR_FINISH: u64 = 60;

/// Default width for all modals.
pub const DEFAULT_MODAL_WIDTH: f32 = 420.;

/// Default width of the sidebar.
pub const DEFAULT_SIDEBAR_WIDTH: f32 = 240.;

/// Image Resize Service
pub const IMAGE_RESIZE_SERVICE: &str = "https://wsrv.nl";

/// Default NIP96 Media Server.
pub const NIP96_SERVER: &str = "https://nostrmedia.com";
