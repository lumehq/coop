pub const APP_NAME: &str = "Coop";
pub const APP_ID: &str = "su.reya.coop";

pub const KEYRING_SERVICE: &str = "Coop Safe Storage";
pub const CLIENT_KEYRING: &str = "Coop Client Keys";
pub const MASTER_KEYRING: &str = "Coop Master Keys";

pub const DEVICE_ANNOUNCEMENT_KIND: u16 = 10044;
pub const DEVICE_REQUEST_KIND: u16 = 4454;
pub const DEVICE_RESPONSE_KIND: u16 = 4455;

/// Bootstrap relays
pub const BOOTSTRAP_RELAYS: [&str; 3] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://purplepag.es",
];

/// Subscriptions
pub const NEW_MESSAGE_SUB_ID: &str = "listen_new_giftwraps";
pub const ALL_MESSAGES_SUB_ID: &str = "listen_all_giftwraps";
pub const DEVICE_SUB_ID: &str = "listen_device_announcement";

/// Image Resizer Service
pub const IMAGE_SERVICE: &str = "https://wsrv.nl";

/// NIP96 Media Server
pub const NIP96_SERVER: &str = "https://nostrmedia.com";
