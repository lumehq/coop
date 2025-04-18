use global::constants::IMAGE_SERVICE;
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
            .map(|picture| {
                format!(
                    "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1&default=npub1zfss807aer0j26mwp2la0ume0jqde3823rmu97ra6sgyyg956e0s6xw445.blossom.band/c30703b48f511c293a9003be8100cdad37b8798b77a1dc3ec6eb8a20443d5dea.png",
                    IMAGE_SERVICE, picture
                )
                .into()
            })
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
