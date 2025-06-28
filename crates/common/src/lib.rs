use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use anyhow::{anyhow, Error, Result};
use gpui::{Image, ImageFormat};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use qrcode_generator::QrCodeEcc;
use reqwest::Client as ReqClient;

pub mod debounced_delay;
pub mod handle_auth;
pub mod profile;

pub async fn verify_nip05(public_key: PublicKey, address: &str) -> Result<bool, Error> {
    let req_client = ReqClient::new();
    let address = Nip05Address::parse(address)?;
    let res = req_client.get(address.url().to_string()).send().await?;
    let json: Value = res.json().await?;
    let verify = nip05::verify_from_json(&public_key, &address, &json);

    Ok(verify)
}

pub async fn nip05_profile(address: &str) -> Result<Nip05Profile, Error> {
    let req_client = ReqClient::new();
    let address = Nip05Address::parse(address)?;
    let res = req_client.get(address.url().to_string()).send().await?;
    let json: Value = res.json().await?;

    if let Ok(profile) = Nip05Profile::from_json(&address, &json) {
        Ok(profile)
    } else {
        Err(anyhow!("Failed to get NIP-05 profile"))
    }
}

pub async fn nip96_upload(client: &Client, server: Url, file: Vec<u8>) -> Result<Url, Error> {
    let signer = client.signer().await?;
    let config = nip96::get_server_config(server.to_owned(), None).await?;
    let url = nip96::upload_data(&signer, &config, file, None, None).await?;

    Ok(url)
}

pub fn room_hash(event: &Event) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut pubkeys: Vec<PublicKey> = vec![];

    // Add all public keys from event
    pubkeys.push(event.pubkey);
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

pub fn string_to_qr(data: &str) -> Option<Arc<Image>> {
    let Ok(bytes) = qrcode_generator::to_png_to_vec_from_str(data, QrCodeEcc::Medium, 256) else {
        return None;
    };

    Some(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
}

pub fn compare<T>(a: &[T], b: &[T]) -> bool
where
    T: Eq + Hash,
{
    let a: HashSet<_> = a.iter().collect();
    let b: HashSet<_> = b.iter().collect();

    a == b
}
