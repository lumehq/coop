use std::sync::OnceLock;

use nostr_sdk::prelude::*;

use crate::constants::{ACCOUNT_PATH, SETTINGS_PATH};

/// Returns the nostr's identifier tag for the account path.
pub fn account_identifier() -> &'static Tag {
    static ACCOUNT_IDENTIFIER: OnceLock<Tag> = OnceLock::new();
    ACCOUNT_IDENTIFIER.get_or_init(|| Tag::identifier(ACCOUNT_PATH))
}

/// Returns the nostr's identifier tag for the settings path.
pub fn settings_identifier() -> &'static Tag {
    static SETTINGS_IDENTIFIER: OnceLock<Tag> = OnceLock::new();
    SETTINGS_IDENTIFIER.get_or_init(|| Tag::identifier(SETTINGS_PATH))
}
