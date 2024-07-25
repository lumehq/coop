use itertools::Itertools;
use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
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

	match client.get_events_of(vec![filter], None).await {
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
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let keyring = Entry::new(&id, "nostr_secret").expect("Unexpected.");

	let password = match keyring.get_password() {
		Ok(pw) => pw,
		Err(_) => return Err("Cancelled".into()),
	};

	let keys = Keys::parse(password).expect("Secret Key is modified, please check again.");
	let signer = NostrSigner::Keys(keys);

	// Set signer
	client.set_signer(Some(signer)).await;

	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	if let Ok(events) = client.get_events_of(vec![inbox], None).await {
		if let Some(event) = events.into_iter().next() {
			for tag in &event.tags {
				if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
					let url = url.to_string();

					if client.add_relay(&url).await.is_ok() {
						println!("Adding relay {} ...", url);

						if client.connect_relay(&url).await.is_ok() {
							println!("Connecting relay {} ...", url);
						}
					}
				}
			}
		}
	}

	tauri::async_runtime::spawn(async move {
		let window = handle.get_webview_window("main").expect("Window is terminated.");
		let state = window.state::<Nostr>();
		let client = &state.client;

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
				if let RelayPoolNotification::Message { message, relay_url } = notification {
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
					} else if let RelayMessage::Auth { challenge } = message {
						match client.auth(challenge, relay_url.clone()).await {
							Ok(..) => {
								println!("Authenticated to {} relay.", relay_url);

								if let Ok(relay) = client.relay(relay_url).await {
									let opts = RelaySendOptions::new().skip_send_confirmation(true);
									if let Err(e) = relay.resubscribe(opts).await {
										println!(
											"Impossible to resubscribe to '{}': {e}",
											relay.url()
										);
									}
								}
							}
							Err(e) => {
								println!("Can't authenticate to '{relay_url}' relay: {e}");
							}
						}
					} else {
						println!("relay message: {}", message.as_json());
					}
				}
				Ok(false)
			})
			.await
	});

	let hex = public_key.to_hex();

	Ok(hex)
}
