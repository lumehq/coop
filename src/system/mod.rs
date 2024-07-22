use std::collections::HashSet;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::stream::{self, StreamExt};
use itertools::Itertools;
use keyring::Entry;
use nostr_sdk::prelude::*;

use crate::common::is_target;

pub mod state;

pub async fn create_account(client: &Client) -> Result<()> {
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

pub async fn connect_account(client: &Client, uri: String) -> Result<()> {
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

pub async fn import_key(client: &Client, nsec: String) -> Result<()> {
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

pub async fn login(client: &Client, public_key: PublicKey) -> Result<PublicKey> {
	let npub = public_key.to_bech32()?;
	let keyring = Entry::new(&npub, "nostr_secret")?;

	let key = keyring.get_password()?;
	let parsed_key = Keys::parse(key)?;
	let signer = NostrSigner::Keys(parsed_key);

	// Set signer
	client.set_signer(Some(signer)).await;

	let incoming = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
	let inbox = Filter::new()
		.kind(Kind::Custom(10050))
		.author(public_key)
		.limit(1);

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

					println!("Connecting to {} ...", relay);
				}
			}
		}
	}

	if let Ok(report) = client
		.reconcile(incoming.clone(), NegentropyOptions::default())
		.await
	{
		let receives = report.received.clone();
		let ids = receives.into_iter().collect::<Vec<_>>();

		let events = client
			.database()
			.query(vec![Filter::new().ids(ids)], Order::Desc)
			.await?;

		let pubkeys = events
			.into_iter()
			.unique_by(|ev| ev.pubkey)
			.map(|ev| ev.pubkey)
			.collect::<Vec<_>>();

		if client
			.reconcile(
				Filter::new().kind(Kind::GiftWrap).pubkeys(pubkeys),
				NegentropyOptions::default(),
			)
			.await
			.is_ok()
		{
			println!("Sync done.")
		}
	}

	if client
		.subscribe(vec![incoming.limit(0)], None)
		.await
		.is_ok()
	{
		println!("Waiting for new message...")
	}

	Ok(public_key)
}

pub async fn update_profile(client: &Client, metadata: Metadata) -> Result<()> {
	let _ = client.set_metadata(&metadata).await?;
	Ok(())
}

pub async fn get_profile(client: &Client, public_key: Option<&PublicKey>) -> Result<Metadata> {
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
		.get_events_of(vec![filter], Some(Duration::from_secs(5)))
		.await?;

	if let Some(event) = events.first() {
		Ok(Metadata::from_json(&event.content)?)
	} else {
		Err(anyhow!("Not found."))
	}
}

pub async fn get_contact_list(client: &Client) -> Result<Vec<Contact>> {
	let list = client.get_contact_list(None).await?;
	Ok(list)
}

pub async fn get_chats(client: &Client) -> Result<Vec<UnsignedEvent>> {
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
		.unique_by(|ev| ev.pubkey)
		.collect::<Vec<_>>())
}

pub async fn preload(client: &Client, public_key: PublicKey) -> Result<()> {
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

pub async fn get_chat_messages(
	client: &Client,
	sender_pk: PublicKey,
) -> Result<Vec<UnsignedEvent>> {
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

pub async fn get_inboxes(client: &Client, public_key: PublicKey) -> Result<Vec<String>> {
	let inbox = Filter::new()
		.kind(Kind::Custom(10050))
		.author(public_key)
		.limit(1);

	let events = client
		.get_events_of(vec![inbox], Some(Duration::from_secs(5)))
		.await?;

	let mut relays = Vec::new();

	if let Some(event) = events.into_iter().next() {
		for tag in &event.tags {
			if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
				let relay = url.to_string();
				if client.add_relay(&relay).await.is_ok() {
					relays.push(relay)
				}
			}
		}
	}
	
	client.connect().await;

	Ok(relays)
}

pub async fn send_message(
	client: &Client,
	receiver: PublicKey,
	message: String,
	relays: Vec<String>,
) -> Result<HashSet<Url>> {
	match client
		.send_private_msg_to(relays, receiver, message.clone(), None)
		.await
	{
		Ok(output) => Ok(output.success),
		Err(_) => Err(anyhow!("Error.")),
	}
}
