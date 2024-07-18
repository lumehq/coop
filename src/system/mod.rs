use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::stream::{self, StreamExt};
use itertools::Itertools;
use keyring::Entry;
use nostr_sdk::prelude::*;

use crate::common::is_target;
use crate::system::state::get_client;

pub mod state;

pub async fn create_account() -> Result<()> {
	let client = get_client().await;
	let keys = Keys::generate();
	let npub = keys.public_key().to_bech32()?;
	let nsec = keys.secret_key().unwrap().to_bech32()?;

	// Save account
	let keyring = Entry::new(&npub, "nostr_secret")?;
	let _ = keyring.set_password(&nsec);

	let signer = NostrSigner::Keys(keys);

	// Update signer
	client.set_signer(Some(signer)).await;

	Ok(())
}

pub async fn connect_account(uri: String) -> Result<()> {
	let client = get_client().await;
	let bunker_uri = NostrConnectURI::parse(uri)?;

	let app_keys = Keys::generate();
	let app_secret = app_keys.secret_key().unwrap().to_string();

	// Get remote user
	let remote_user = bunker_uri.signer_public_key().unwrap();
	let remote_npub = remote_user.to_bech32()?;

	let signer = Nip46Signer::new(bunker_uri, app_keys, Duration::from_secs(120), None).await?;

	let keyring = Entry::new(&remote_npub, "nostr_secret")?;
	let _ = keyring.set_password(&app_secret);

	// Update signer
	let _ = client.set_signer(Some(signer.into())).await;

	Ok(())
}

pub async fn import_key(nsec: String) -> Result<()> {
	let client = get_client().await;
	let secret_key = SecretKey::parse(nsec.clone())?;
	let keys = Keys::new(secret_key);
	let npub = keys.public_key().to_bech32()?;

	let keyring = Entry::new(&npub, "nostr_secret")?;
	let _ = keyring.set_password(&nsec);

	// Update signer
	let signer = NostrSigner::Keys(keys);
	let _ = client.set_signer(Some(signer)).await;

	Ok(())
}

pub async fn login(public_key: PublicKey) -> Result<String> {
	let npub = public_key.to_bech32()?;
	let hex = public_key.to_hex();
	let keyring = Entry::new(&npub, "nostr_secret")?;

	let key = keyring.get_password()?;
	let parsed_key = Keys::parse(key)?;

	let client = get_client().await;
	let signer = NostrSigner::Keys(parsed_key);

	// Set signer
	client.set_signer(Some(signer)).await;

	// Connect to inbox relay
	let inbox = Filter::new()
		.kind(Kind::Custom(10050))
		.author(public_key)
		.limit(1);
	let incoming = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	if let Ok(events) = client
		.get_events_of(vec![inbox], Some(Duration::from_secs(8)))
		.await
	{
		if let Some(event) = events.into_iter().next() {
			for tag in &event.tags {
				if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
					let relay = url.to_string();
					let _ = client.add_relay(&relay).await;
					let _ = client.connect_relay(&relay).await;
				}
			}
		}
	}

	if client
		.reconcile(incoming.clone(), NegentropyOptions::default())
		.await
		.is_ok()
	{
		println!("Sync done.")
	}

	if client
		.subscribe(vec![incoming.limit(0)], None)
		.await
		.is_ok()
	{
		println!("Waiting for new message...")
	}

	Ok(hex)
}

pub async fn update_profile(metadata: Metadata) -> Result<()> {
	let client = get_client().await;
	let _ = client.set_metadata(&metadata).await?;

	Ok(())
}

pub async fn get_profile(public_key: Option<&PublicKey>) -> Result<Metadata> {
	let client = get_client().await;

	let public_key = match public_key {
		Some(pk) => *pk,
		None => {
			let signer = client.signer().await?;
			signer.public_key().await?
		}
	};

	let filter = Filter::new()
		.author(public_key)
		.kind(Kind::Metadata)
		.limit(1);

	let events = client
		.get_events_of(vec![filter], Some(Duration::from_secs(2)))
		.await?;

	if let Some(event) = events.first() {
		Ok(Metadata::from_json(&event.content)?)
	} else {
		Err(anyhow!("Not found."))
	}
}

pub async fn get_chats() -> Result<Vec<UnsignedEvent>> {
	let client = get_client().await;
	let signer = client.signer().await?;
	let public_key = signer.public_key().await?;

	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	let events = client.database().query(vec![filter], Order::Desc).await?;

	let rumors = stream::iter(events)
		.filter_map(|ev| async move {
			if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(&ev).await {
				if rumor.kind == Kind::PrivateDirectMessage {
					return Some(rumor);
				}
			}
			None
		})
		.collect::<Vec<_>>()
		.await;

	Ok(rumors
		.into_iter()
		.unique_by(|ev| ev.pubkey.to_hex())
		.collect::<Vec<_>>())
}

pub async fn preload(public_key: PublicKey) -> Result<()> {
	let client = get_client().await;
	let signer = client.signer().await?;
	let receiver_pk = signer.public_key().await?;

	let messages = Filter::new()
		.kind(Kind::GiftWrap)
		.pubkeys(vec![public_key, receiver_pk])
		.limit(100);

	if client
		.reconcile(messages, NegentropyOptions::default())
		.await
		.is_ok()
	{
		println!("preloaded.")
	}
	Ok(())
}

pub async fn get_chat_messages(sender_pk: PublicKey) -> Result<Vec<UnsignedEvent>> {
	let client = get_client().await;
	let database = client.database();
	let signer = client.signer().await?;
	let receiver_pk = signer.public_key().await?;

	let filter = Filter::new()
		.kind(Kind::GiftWrap)
		.pubkeys(vec![receiver_pk, sender_pk]);

	let messages = Filter::new()
		.kind(Kind::GiftWrap)
		.pubkey(sender_pk)
		.limit(0);

	let subscription_id = SubscriptionId::new(format!("channel_{}", sender_pk.to_hex()));
	client
		.subscribe_with_id(subscription_id, vec![messages], None)
		.await?;

	let events = database.query(vec![filter], Order::Desc).await?;

	let rumors = stream::iter(events)
		.filter_map(|ev| async move {
			if let Ok(UnwrappedGift { rumor, sender }) = client.unwrap_gift_wrap(&ev).await {
				if rumor.kind == Kind::PrivateDirectMessage
					&& (sender == sender_pk || is_target(&sender_pk, &rumor.tags))
				{
					return Some(rumor);
				}
			}
			None
		})
		.collect::<Vec<_>>()
		.await;

	Ok(rumors
		.into_iter()
		.sorted_by_key(|ev| ev.created_at)
		.collect::<Vec<_>>())
}

pub async fn get_inboxes(public_key: PublicKey) -> Result<Vec<String>> {
	let client = get_client().await;
	let inbox = Filter::new()
		.kind(Kind::Custom(10050))
		.author(public_key)
		.limit(1);
	let mut relays = Vec::new();

	let events = client
		.get_events_of(vec![inbox], Some(Duration::from_secs(8)))
		.await?;

	if let Some(event) = events.into_iter().next() {
		for tag in &event.tags {
			if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
				relays.push(url.to_string())
			}
		}
	}

	for relay in &relays {
		if client.add_relay(relay).await.is_ok() {
			println!("Adding inbox relay: {}", relay);
		}
	}

	Ok(relays)
}

pub async fn send_message(
	receiver: PublicKey,
	message: String,
	relays: Vec<String>,
) -> Result<UnsignedEvent> {
	let client = get_client().await;
	let signer = client.signer().await?;
	let public_key = signer.public_key().await?;

	for relay in &relays {
		let _ = client.connect_relay(relay).await;
	}

	// TODO: send message to inbox relays only.
	client
		.send_private_msg(receiver, message.clone(), None)
		.await?;

	let rumor = EventBuilder::private_msg_rumor(receiver, message, None);
	Ok(rumor.to_unsigned_event(public_key))
}
