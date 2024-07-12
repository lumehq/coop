use std::time::Duration;

use futures::stream::{self, StreamExt};
use itertools::Itertools;
use keyring::Entry;
use nostr_sdk::prelude::*;

use crate::system::state::get_client;

pub mod radio;
pub mod state;

pub async fn login(public_key: PublicKey) -> Result<String, String> {
	let npub = public_key.to_bech32().unwrap();
	let hex = public_key.to_hex();
	let keyring = Entry::new(&npub, "nostr_secret").unwrap();

	let keys = match keyring.get_password() {
		Ok(pw) => Keys::parse(pw).unwrap(),
		Err(_) => return Err("Cancelled".into()),
	};

	let client = get_client().await;
	let signer = NostrSigner::Keys(keys);

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
		if let Some(event) = events.first() {
			for tag in &event.tags {
				if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
					let relay = url.to_string();
					println!("connecting to inbox relay: {}", relay);
					if client.add_relay(&relay).await.is_ok() {
						client.connect_relay(&relay).await.unwrap()
					}
				}
			}
		}
	}

	if client
		.reconcile(incoming, NegentropyOptions::default())
		.await
		.is_ok()
	{
		println!("Sync done.")
	}

	Ok(hex)
}

pub async fn get_profile(public_key: Option<&PublicKey>) -> Result<Metadata, String> {
	let client = get_client().await;

	let public_key = match public_key {
		Some(pk) => pk.to_owned(),
		None => {
			let signer = client.signer().await.unwrap();
			signer.public_key().await.unwrap()
		}
	};

	let filter = Filter::new()
		.author(public_key)
		.kind(Kind::Metadata)
		.limit(1);

	match client
		.get_events_of(vec![filter], Some(Duration::from_secs(2)))
		.await
	{
		Ok(events) => {
			if let Some(event) = events.first() {
				match Metadata::from_json(&event.content) {
					Ok(val) => Ok(val),
					Err(err) => Err(err.to_string()),
				}
			} else {
				Err("Not found.".into())
			}
		}
		Err(err) => Err(err.to_string()),
	}
}

pub async fn get_chats() -> Result<Vec<UnsignedEvent>, String> {
	let client = get_client().await;
	let signer = client.signer().await.unwrap();
	let public_key = signer.public_key().await.unwrap();

	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	match client.database().query(vec![filter], Order::Desc).await {
		Ok(events) => {
			let rumors = stream::iter(events)
				.filter_map(|ev| async move {
					match client.unwrap_gift_wrap(&ev).await {
						Ok(UnwrappedGift { rumor, .. }) => {
							if rumor.kind == Kind::PrivateDirectMessage {
								Some(rumor)
							} else {
								None
							}
						}
						Err(_) => None,
					}
				})
				.collect::<Vec<_>>()
				.await;

			let uniqs = rumors
				.into_iter()
				.unique_by(|ev| ev.pubkey.to_hex())
				.collect::<Vec<_>>();

			Ok(uniqs)
		}
		Err(err) => Err(err.to_string()),
	}
}
