use crate::constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID, NIP96_SERVER};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use rnglib::{Language, RNG};
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    time::Duration,
};

pub async fn signer_public_key(client: &Client) -> anyhow::Result<PublicKey, anyhow::Error> {
    let signer = client.signer().await?;
    let public_key = signer.get_public_key().await?;

    Ok(public_key)
}

pub async fn preload(client: &Client, public_key: PublicKey) -> anyhow::Result<(), anyhow::Error> {
    let subscription = Filter::new()
        .kind(Kind::ContactList)
        .author(public_key)
        .limit(1);

    // Get contact list
    _ = client.sync(subscription, &SyncOptions::default()).await;

    let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
    let new_message_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

    // Create a filter for getting all gift wrapped events send to current user
    let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

    // Create a filter for getting new message
    let new_message = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(public_key)
        .limit(0);

    // Subscribe for all messages
    _ = client
        .subscribe_with_id(
            all_messages_sub_id,
            all_messages,
            Some(
                SubscribeAutoCloseOptions::default()
                    .exit_policy(ReqExitPolicy::WaitDurationAfterEOSE(Duration::from_secs(3))),
            ),
        )
        .await;

    // Subscribe for new message
    _ = client
        .subscribe_with_id(new_message_sub_id, new_message, None)
        .await;

    Ok(())
}

pub async fn nip96_upload(client: &Client, file: Vec<u8>) -> anyhow::Result<Url, anyhow::Error> {
    let signer = client.signer().await?;
    let server_url = Url::parse(NIP96_SERVER)?;

    let config: ServerConfig = nip96::get_server_config(server_url, None).await?;
    let url = nip96::upload_data(&signer, &config, file, None, None).await?;

    Ok(url)
}

pub fn room_hash(event: &Event) -> u64 {
    let pubkeys: Vec<&PublicKey> = event.tags.public_keys().unique().collect();
    let mut hasher = DefaultHasher::new();
    // Generate unique hash
    pubkeys.hash(&mut hasher);
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
