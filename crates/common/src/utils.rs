use crate::constants::NIP96_SERVER;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use rnglib::{Language, RNG};
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

pub async fn signer_public_key(client: &Client) -> anyhow::Result<PublicKey, anyhow::Error> {
    let signer = client.signer().await?;
    let public_key = signer.get_public_key().await?;

    Ok(public_key)
}

pub async fn nip96_upload(client: &Client, file: Vec<u8>) -> anyhow::Result<Url, anyhow::Error> {
    let signer = client.signer().await?;
    let server_url = Url::parse(NIP96_SERVER)?;

    let config: ServerConfig = nip96::get_server_config(server_url, None).await?;
    let url = nip96::upload_data(&signer, &config, file, None, None).await?;

    Ok(url)
}

pub fn room_hash(event: &Event) -> u64 {
    let mut pubkeys: Vec<&PublicKey> = vec![];
    pubkeys.push(&event.pubkey);
    pubkeys.extend(
        event
            .tags
            .public_keys()
            .unique()
            .sorted()
            .collect::<Vec<_>>(),
    );

    let mut hasher = DefaultHasher::new();
    // Generate unique hash
    pubkeys
        .into_iter()
        .unique()
        .sorted()
        .collect::<Vec<_>>()
        .hash(&mut hasher);

    hasher.finish()
}

pub fn random_name(length: usize) -> String {
    let rng = RNG::from(&Language::Roman);
    rng.generate_names(length, true).join("-").to_lowercase()
}

pub fn compare<T>(a: &[T], b: &[T]) -> bool
where
    T: Eq + Hash,
{
    let a: HashSet<_> = a.iter().collect();
    let b: HashSet<_> = b.iter().collect();

    a == b
}
