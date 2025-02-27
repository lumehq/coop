pub const APP_NAME: &str = "Coop";
pub const APP_ID: &str = "su.reya.coop";

/// Bootstrap relays
pub const BOOTSTRAP_RELAYS: [&str; 3] = [
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://purplepag.es",
];

/// Subscriptions
pub const NEW_MESSAGE_SUB_ID: &str = "listen_new_giftwraps";
pub const ALL_MESSAGES_SUB_ID: &str = "listen_all_giftwraps";

/// Image Resizer Service
pub const IMAGE_SERVICE: &str = "https://wsrv.nl";

/// NIP96 Media Server
pub const NIP96_SERVER: &str = "https://nostrmedia.com";

/// Updater Public Key
pub const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDkxM0EzQTQyRTBBMENENTYKUldSV3phRGdRam82a1dtU0JqYll4VnBaVUpSWUxCWlVQbnRkUnNERS96MzFMWDhqNW5zOXplMEwK";
/// Updater Server URL
pub const UPDATER_URL: &str =
    "https://cdn.crabnebula.app/update/lume/coop/{{target}}-{{arch}}/{{current_version}}";
