use itertools::Itertools;
use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;
use std::{collections::HashSet, str::FromStr};
use tauri::State;

use crate::Nostr;

#[tauri::command]
pub fn get_accounts() -> Vec<String> {
	let search = Search::new().expect("Unexpected.");
	let results = search.by_user("nostr_secret");
	let list = List::list_credentials(&results, Limit::All);
	let accounts: HashSet<String> =
		list.split_whitespace().filter(|v| v.starts_with("npub1")).map(String::from).collect();

	accounts.into_iter().collect()
}

#[tauri::command]
pub async fn get_profile(id: String, state: State<'_, Nostr>) -> Result<String, ()> {
	let client = &state.client;
	let public_key = PublicKey::from_str(&id).unwrap();
	let filter = Filter::new().author(public_key).kind(Kind::Metadata).limit(1);

	let events = client.get_events_of(vec![filter], None).await.unwrap();

	if let Some(event) = events.first() {
		Ok(Metadata::from_json(&event.content).unwrap().as_json())
	} else {
		Ok(Metadata::new().as_json())
	}
}

#[tauri::command]
pub async fn login(id: String, state: State<'_, Nostr>) -> Result<(), String> {
	let client = &state.client;
	let keyring = Entry::new(&id, "nostr_secret").expect("Unexpected.");

	let password = match keyring.get_password() {
		Ok(pw) => pw,
		Err(_) => return Err("Cancelled".into()),
	};

	let keys = Keys::parse(password).expect("Secret Key is modified, please check again.");
	let signer = NostrSigner::Keys(keys);

	// Set signer
	client.set_signer(Some(signer)).await;

	let public_key = PublicKey::from_str(&id).unwrap();
	let incoming = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	if let Ok(events) = client.get_events_of(vec![inbox], None).await {
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

	if let Ok(report) = client.reconcile(incoming.clone(), NegentropyOptions::default()).await {
		let receives = report.received.clone();
		let ids = receives.into_iter().collect::<Vec<_>>();

		if let Ok(events) = client.database().query(vec![Filter::new().ids(ids)], Order::Desc).await
		{
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
	}

	if client.subscribe(vec![incoming.limit(0)], None).await.is_ok() {
		println!("Waiting for new message...")
	}

	Ok(())
}
