use itertools::Itertools;
use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;
use serde::Serialize;
use std::{collections::HashSet, time::Duration};
use tauri::{Emitter, Manager, State};

use crate::Nostr;

#[derive(Clone, Serialize)]
struct Payload {
	event: String,
	sender: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_accounts() -> Vec<String> {
	let search = Search::new().expect("Unexpected.");
	let results = search.by_user("nostr_secret");
	let list = List::list_credentials(&results, Limit::All);
	let accounts: HashSet<String> =
		list.split_whitespace().filter(|v| v.starts_with("npub1")).map(String::from).collect();

	accounts.into_iter().collect()
}

#[tauri::command]
#[specta::specta]
pub async fn get_profile(id: String, state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let filter = Filter::new().author(public_key).kind(Kind::Metadata).limit(1);

	match client.get_events_of(vec![filter], Some(Duration::from_secs(1))).await {
		Ok(events) => {
			if let Some(event) = events.first() {
				Ok(Metadata::from_json(&event.content).unwrap_or(Metadata::new()).as_json())
			} else {
				Ok(Metadata::new().as_json())
			}
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn login(
	id: String,
	state: State<'_, Nostr>,
	handle: tauri::AppHandle,
) -> Result<String, String> {
	let client = &state.client;
	let keyring = Entry::new(&id, "nostr_secret").expect("Unexpected.");

	let password = match keyring.get_password() {
		Ok(pw) => pw,
		Err(_) => return Err("Cancelled".into()),
	};

	let id_clone = id.clone();
	let keys = Keys::parse(password).expect("Secret Key is modified, please check again.");
	let signer = NostrSigner::Keys(keys);

	// Set signer
	client.set_signer(Some(signer)).await;

	tauri::async_runtime::spawn(async move {
		let window = handle.get_webview_window("main").unwrap();
		let state = window.state::<Nostr>();
		let client = &state.client;

		let public_key = PublicKey::parse(&id_clone).unwrap();
		let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

		if let Ok(events) = client.get_events_of(vec![inbox], None).await {
			if let Some(event) = events.into_iter().next() {
				for tag in &event.tags {
					if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
						let opts = RelayOptions::new().retry_sec(5);
						let url = url.to_string();

						if client.add_relay_with_opts(&url, opts).await.is_ok() {
							println!("Adding relay {} ...", url);

							if client.connect_relay(&url).await.is_ok() {
								println!("Connecting relay {} ...", url);
							}
						}
					}
				}
			}
		}

		let old = Filter::new().kind(Kind::GiftWrap).pubkey(public_key).until(Timestamp::now());
		let new = Filter::new().kind(Kind::GiftWrap).pubkey(public_key).limit(0);

		if let Ok(report) = client.reconcile(old, NegentropyOptions::default()).await {
			let receives = report.received.clone();
			let ids = receives.into_iter().collect::<Vec<_>>();

			if let Ok(events) =
				client.database().query(vec![Filter::new().ids(ids)], Order::Desc).await
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
		};

		if client.subscribe(vec![new], None).await.is_ok() {
			println!("Waiting for new message...")
		};

		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Message { message, .. } = notification {
					if let RelayMessage::Event { event, .. } = message {
						if event.kind == Kind::GiftWrap {
							if let Ok(UnwrappedGift { rumor, sender }) =
								client.unwrap_gift_wrap(&event).await
							{
								window
									.emit(
										"event",
										Payload { event: rumor.as_json(), sender: sender.to_hex() },
									)
									.unwrap();
							}
						}
					}
				}
				Ok(false)
			})
			.await
	});

	let public_key = PublicKey::parse(&id).unwrap();
	let hex = public_key.to_hex();

	Ok(hex)
}
