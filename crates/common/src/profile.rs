use crate::{constants::IMAGE_SERVICE, utils::shorted_public_key};
use nostr_sdk::prelude::*;

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
    pub fn avatar(&self) -> String {
        if let Some(picture) = &self.metadata.picture {
            if picture.len() > 1 {
                format!(
                    "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1",
                    IMAGE_SERVICE, picture
                )
            } else {
                "brand/avatar.png".into()
            }
        } else {
            "brand/avatar.png".into()
        }
    }

    /// Get contact's name, fallback to public key as shorted format
    pub fn name(&self) -> String {
        if let Some(display_name) = &self.metadata.display_name {
            if !display_name.is_empty() {
                return display_name.clone();
            }
        }

        if let Some(name) = &self.metadata.name {
            if !name.is_empty() {
                return name.clone();
            }
        }

        shorted_public_key(self.public_key)
    }

    /// Get contact's metadata
    pub fn metadata(&mut self) -> &Metadata {
        &self.metadata
    }

    /// Set contact's metadata
    pub fn set_metadata(&mut self, metadata: &Metadata) {
        self.metadata = metadata.clone()
    }
}
