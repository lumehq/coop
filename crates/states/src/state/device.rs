use std::sync::Arc;

use nostr_sdk::prelude::*;

#[derive(Debug, Clone, Default)]
pub struct Device {
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Client Key that used for communication between devices
    pub client: Option<Arc<dyn NostrSigner>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption key used for encryption and decryption instead of the user's identity
    pub encryption: Option<Arc<dyn NostrSigner>>,
}

impl Device {
    pub fn new() -> Self {
        Self {
            client: None,
            encryption: None,
        }
    }

    pub fn set_client<T>(&mut self, keys: T)
    where
        T: NostrSigner,
    {
        self.client = Some(Arc::new(keys));
    }

    pub fn set_encryption<T>(&mut self, keys: T)
    where
        T: NostrSigner,
    {
        self.encryption = Some(Arc::new(keys));
    }
}
