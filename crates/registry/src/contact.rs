use common::{constants::IMAGE_SERVICE, utils::shorted_public_key};
use nostr_sdk::prelude::*;

#[derive(Debug, Clone)]
pub struct Contact {
    public_key: PublicKey,
    metadata: Metadata,
}

impl PartialEq for Contact {
    fn eq(&self, other: &Self) -> bool {
        self.public_key() == other.public_key()
    }
}

impl Contact {
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

    /// Set contact's metadata
    pub fn metadata(&mut self, metadata: &Metadata) {
        self.metadata = metadata.clone()
    }

    /// Get contact's avatar
    pub fn avatar(&self) -> String {
        if let Some(picture) = &self.metadata.picture {
            format!(
                "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1",
                IMAGE_SERVICE, picture
            )
        } else {
            "brand/avatar.png".into()
        }
    }

    /// Get contact's name
    /// Fallback to public key as shorted format
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
}
