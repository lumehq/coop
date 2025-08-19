use std::sync::Arc;

use anyhow::{anyhow, Error};
use global::constants::IMAGE_RESIZE_SERVICE;
use gpui::{Image, ImageFormat, SharedString};
use nostr_sdk::prelude::*;
use qrcode_generator::QrCodeEcc;

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

        shorten_pubkey(self.public_key(), 4)
    }
}

pub trait TextUtils {
    fn to_public_key(&self) -> Result<PublicKey, Error>;
    fn to_qr(&self) -> Option<Arc<Image>>;
}

impl TextUtils for String {
    fn to_public_key(&self) -> Result<PublicKey, Error> {
        if self.starts_with("nprofile1") {
            Ok(Nip19Profile::from_bech32(self)?.public_key)
        } else if self.starts_with("npub1") {
            Ok(PublicKey::parse(self)?)
        } else {
            Err(anyhow!("Invalid public key"))
        }
    }

    fn to_qr(&self) -> Option<Arc<Image>> {
        let ecc = QrCodeEcc::Low;
        let format = ImageFormat::Png;
        let size = 256;

        let Ok(bytes) = qrcode_generator::to_png_to_vec_from_str(self, ecc, size) else {
            return None;
        };

        Some(Arc::new(Image::from_bytes(format, bytes)))
    }
}

impl TextUtils for &str {
    fn to_public_key(&self) -> Result<PublicKey, Error> {
        if self.starts_with("nprofile1") {
            Ok(Nip19Profile::from_bech32(self)?.public_key)
        } else if self.starts_with("npub1") {
            Ok(PublicKey::parse(self)?)
        } else {
            Err(anyhow!("Invalid public key"))
        }
    }

    fn to_qr(&self) -> Option<Arc<Image>> {
        let Ok(bytes) = qrcode_generator::to_png_to_vec_from_str(self, QrCodeEcc::Medium, 256)
        else {
            return None;
        };

        Some(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
    }
}

pub fn shorten_pubkey(public_key: PublicKey, len: usize) -> SharedString {
    let Ok(pubkey) = public_key.to_bech32();

    format!(
        "{}:{}",
        &pubkey[0..(len + 1)],
        &pubkey[pubkey.len() - len..]
    )
    .into()
}
