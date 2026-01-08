use std::sync::Arc;

use nostr_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelayState {
    #[default]
    Initial,
    NotSet,
    Set,
}

impl RelayState {
    pub fn is_initial(&self) -> bool {
        matches!(self, RelayState::Initial)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Identity {
    /// The public key of the account
    pub public_key: Option<PublicKey>,

    /// Decoupled encryption key
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    dekey: Option<Arc<dyn NostrSigner>>,

    /// Status of the current user NIP-65 relays
    relay_list: RelayState,

    /// Status of the current user NIP-17 relays
    messaging_relays: RelayState,
}

impl AsRef<Identity> for Identity {
    fn as_ref(&self) -> &Identity {
        self
    }
}

impl Identity {
    pub fn new() -> Self {
        Self {
            public_key: None,
            dekey: None,
            relay_list: RelayState::default(),
            messaging_relays: RelayState::default(),
        }
    }

    /// Sets the state of the NIP-65 relays.
    pub fn set_relay_list_state(&mut self, state: RelayState) {
        self.relay_list = state;
    }

    /// Returns the state of the NIP-65 relays.
    pub fn relay_list_state(&self) -> RelayState {
        self.relay_list
    }

    /// Sets the state of the NIP-17 relays.
    pub fn set_messaging_relays_state(&mut self, state: RelayState) {
        self.messaging_relays = state;
    }

    /// Returns the state of the NIP-17 relays.
    pub fn messaging_relays_state(&self) -> RelayState {
        self.messaging_relays
    }

    /// Returns the decoupled encryption key.
    pub fn dekey(&self) -> Option<Arc<dyn NostrSigner>> {
        self.dekey.clone()
    }

    /// Sets the decoupled encryption key.
    pub fn set_dekey<S>(&mut self, dekey: S)
    where
        S: NostrSigner + 'static,
    {
        self.dekey = Some(Arc::new(dekey));
    }

    /// Force getting the public key of the identity.
    ///
    /// Panics if the public key is not set.
    pub fn public_key(&self) -> PublicKey {
        self.public_key.unwrap()
    }

    /// Returns true if the identity has a public key.
    pub fn has_public_key(&self) -> bool {
        self.public_key.is_some()
    }

    /// Sets the public key of the identity.
    pub fn set_public_key(&mut self, public_key: PublicKey) {
        self.public_key = Some(public_key);
    }

    /// Unsets the public key of the identity.
    pub fn unset_public_key(&mut self) {
        self.public_key = None;
    }
}
