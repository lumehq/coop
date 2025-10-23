use std::sync::Arc;

use nostr_sdk::prelude::*;

#[derive(Debug, Clone, Default)]
pub struct Device {
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// The client keys that used for communication between devices
    pub client_keys: Option<Arc<dyn NostrSigner>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// The encryption keys that used for encryption and decryption
    pub encryption_keys: Option<Arc<dyn NostrSigner>>,
}

impl Device {
    pub fn new() -> Self {
        Self {
            client_keys: None,
            encryption_keys: None,
        }
    }
}
