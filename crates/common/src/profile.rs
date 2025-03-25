use global::constants::IMAGE_SERVICE;
use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NostrProfile {
    pub public_key: PublicKey,
    pub avatar: SharedString,
    pub name: SharedString,
}

impl NostrProfile {
    pub fn new(public_key: PublicKey, metadata: Metadata) -> Self {
        let name = Self::extract_name(&public_key, &metadata);
        let avatar = Self::extract_avatar(&metadata);

        Self {
            public_key,
            name,
            avatar,
        }
    }

    fn extract_avatar(metadata: &Metadata) -> SharedString {
        metadata
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
            .unwrap_or_else(|| "brand/avatar.jpg".into())
    }

    fn extract_name(public_key: &PublicKey, metadata: &Metadata) -> SharedString {
        if let Some(display_name) = metadata.display_name.as_ref() {
            if !display_name.is_empty() {
                return display_name.into();
            }
        }

        if let Some(name) = metadata.name.as_ref() {
            if !name.is_empty() {
                return name.into();
            }
        }

        let pubkey = public_key.to_hex();

        format!("{}:{}", &pubkey[0..4], &pubkey[pubkey.len() - 4..]).into()
    }
}
