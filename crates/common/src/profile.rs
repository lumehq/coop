use gpui::{SharedString, SharedUri};
use nostr_sdk::prelude::*;

use crate::constants::IMAGE_SERVICE;

#[derive(Debug, Clone)]
pub struct NostrProfile {
    public_key: PublicKey,
    metadata: Metadata,
}

impl AsRef<PublicKey> for NostrProfile {
    fn as_ref(&self) -> &PublicKey {
        &self.public_key
    }
}

impl AsRef<Metadata> for NostrProfile {
    fn as_ref(&self) -> &Metadata {
        &self.metadata
    }
}

impl Eq for NostrProfile {}

impl PartialEq for NostrProfile {
    fn eq(&self, other: &Self) -> bool {
        self.public_key() == other.public_key()
    }
}

impl NostrProfile {
    pub fn new(public_key: PublicKey, metadata: Metadata) -> Self {
        Self {
            public_key,
            metadata,
        }
    }

    /// Get contact's public key
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    /// Get contact's avatar
    pub fn avatar(&self) -> SharedUri {
        self.metadata
            .picture
            .as_ref()
            .filter(|picture| !picture.is_empty())
            .map(|picture| {
                format!(
                    "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1",
                    IMAGE_SERVICE, picture
                )
                .into()
            })
            .unwrap_or_else(|| "brand/avatar.png".into())
    }

    /// Get contact's name, fallback to public key as shorted format
    pub fn name(&self) -> SharedString {
        if let Some(display_name) = &self.metadata.display_name {
            if !display_name.is_empty() {
                return display_name.into();
            }
        }

        if let Some(name) = &self.metadata.name {
            if !name.is_empty() {
                return name.into();
            }
        }

        let pubkey = self.public_key.to_hex();

        format!("{}:{}", &pubkey[0..4], &pubkey[pubkey.len() - 4..]).into()
    }

    /// Get contact's metadata
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Set contact's metadata
    pub fn set_metadata(&mut self, metadata: Metadata) {
        self.metadata = metadata;
    }
}
