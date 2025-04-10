use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use anyhow::Context;
use global::constants::NIP96_SERVER;
use gpui::Image;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use qrcode_generator::QrCodeEcc;
use rnglib::{Language, RNG};

pub mod profile;

pub async fn nip96_upload(client: &Client, file: Vec<u8>) -> anyhow::Result<Url, anyhow::Error> {
    let signer = client.signer().await?;
    let server_url = Url::parse(NIP96_SERVER)?;

    let config: ServerConfig = nip96::get_server_config(server_url, None).await?;
    let url = nip96::upload_data(&signer, &config, file, None, None).await?;

    Ok(url)
}

pub fn room_hash(event: &Event) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut pubkeys: Vec<&PublicKey> = vec![];

    // Add all public keys from event
    pubkeys.push(&event.pubkey);
    pubkeys.extend(event.tags.public_keys().collect::<Vec<_>>());

    // Generate unique hash
    pubkeys
        .into_iter()
        .unique()
        .sorted()
        .collect::<Vec<_>>()
        .hash(&mut hasher);

    hasher.finish()
}

pub fn device_pubkey(event: &Event) -> Result<PublicKey, anyhow::Error> {
    let n_tag = event.tags.find(TagKind::custom("n")).context("Invalid")?;
    let hex = n_tag.content().context("Invalid")?;
    let pubkey = PublicKey::parse(hex)?;

    Ok(pubkey)
}

pub fn random_name(length: usize) -> String {
    let rng = RNG::from(&Language::Roman);
    rng.generate_names(length, true).join("-").to_lowercase()
}

pub fn create_qr(data: &str) -> Result<Arc<Image>, anyhow::Error> {
    let qr = qrcode_generator::to_png_to_vec_from_str(data, QrCodeEcc::Medium, 256)?;
    let img = Arc::new(Image {
        format: gpui::ImageFormat::Png,
        bytes: qr.clone(),
        id: 1,
    });

    Ok(img)
}

pub fn compare<T>(a: &[T], b: &[T]) -> bool
where
    T: Eq + Hash,
{
    let a: HashSet<_> = a.iter().collect();
    let b: HashSet<_> = b.iter().collect();

    a == b
}
