use gpui::SharedString;
use nostr_sdk::prelude::*;

pub trait SharedProfile {
    fn shared_avatar(&self) -> SharedString;
    fn shared_name(&self) -> SharedString;
}

impl SharedProfile for Profile {
    fn shared_avatar(&self) -> SharedString {
        self.metadata()
            .picture
            .as_ref()
            .filter(|picture| !picture.is_empty())
            .map(|picture| picture.into())
            .unwrap_or_else(|| "brand/avatar.png".into())
    }

    fn shared_name(&self) -> SharedString {
        if let Some(display_name) = self.metadata().display_name.as_ref() {
            if !display_name.is_empty() {
                return display_name.into();
            }
        }

        if let Some(name) = self.metadata().name.as_ref() {
            if !name.is_empty() {
                return name.into();
            }
        }

        let pubkey = self.public_key().to_hex();

        format!("{}:{}", &pubkey[0..4], &pubkey[pubkey.len() - 4..]).into()
    }
}
