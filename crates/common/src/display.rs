use global::constants::IMAGE_RESIZE_SERVICE;
use gpui::SharedString;
use nostr_sdk::prelude::*;

const FALLBACK_IMG: &str = "https://image.nostr.build/c30703b48f511c293a9003be8100cdad37b8798b77a1dc3ec6eb8a20443d5dea.png";

pub trait DisplayProfile {
    fn avatar_url(&self, proxy: bool) -> SharedString;
    fn display_name(&self) -> SharedString;
}

impl DisplayProfile for Profile {
    fn avatar_url(&self, proxy: bool) -> SharedString {
        self.metadata()
            .picture
            .as_ref()
            .filter(|picture| !picture.is_empty())
            .map(|picture| {
                if proxy {
                    format!(
                        "{IMAGE_RESIZE_SERVICE}/?url={picture}&w=100&h=100&fit=cover&mask=circle&default={FALLBACK_IMG}&n=-1"
                    )
                    .into()
                } else {
                    picture.into()
                }
            })
            .unwrap_or_else(|| "brand/avatar.png".into())
    }

    fn display_name(&self) -> SharedString {
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

        let Ok(pubkey) = self.public_key().to_bech32();

        format!("{}:{}", &pubkey[0..5], &pubkey[pubkey.len() - 4..]).into()
    }
}
