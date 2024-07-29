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
pub async fn get_metadata(id: String, state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let filter = Filter::new().author(public_key).kind(Kind::Metadata).limit(1);

	match client.get_events_of(vec![filter], Some(Duration::from_secs(2))).await {
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
pub async fn create_account(
	name: String,
	picture: String,
	state: State<'_, Nostr>,
) -> Result<(), String> {
	let client = &state.client;
	let keys = Keys::generate();
	let npub = keys.public_key().to_bech32().map_err(|e| e.to_string())?;
	let nsec = keys.secret_key().unwrap().to_bech32().map_err(|e| e.to_string())?;

	// Save account
	let keyring = Entry::new(&npub, "nostr_secret").unwrap();
	let _ = keyring.set_password(&nsec);

	let signer = NostrSigner::Keys(keys);

	// Update signer
	client.set_signer(Some(signer)).await;

	// Update metadata
	let url = Url::parse(&picture).map_err(|e| e.to_string())?;
	let metadata = Metadata::new().display_name(name).picture(url);

	match client.set_metadata(&metadata).await {
		Ok(_) => Ok(()),
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn import_key(
	nsec: &str,
	password: &str,
	state: State<'_, Nostr>,
) -> Result<String, String> {
	let secret_key = if nsec.starts_with("ncryptsec") {
		let encrypted_key = EncryptedSecretKey::from_bech32(nsec).unwrap();
		encrypted_key.to_secret_key(password).map_err(|err| err.to_string())
	} else {
		SecretKey::from_bech32(nsec).map_err(|err| err.to_string())
	};

	match secret_key {
		Ok(val) => {
			let nostr_keys = Keys::new(val);
			let npub = nostr_keys.public_key().to_bech32().unwrap();
			let nsec = nostr_keys.secret_key().unwrap().to_bech32().unwrap();

			let keyring = Entry::new(&npub, "nostr_secret").unwrap();
			let _ = keyring.set_password(&nsec);

			let signer = NostrSigner::Keys(nostr_keys);
			let client = &state.client;

			// Update client's signer
			client.set_signer(Some(signer)).await;

			Ok(npub)
		}
		Err(msg) => Err(msg),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn connect_account(uri: &str, state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;

	match NostrConnectURI::parse(uri) {
		Ok(bunker_uri) => {
			let app_keys = Keys::generate();
			let app_secret = app_keys.secret_key().unwrap().to_string();

			// Get remote user
			let remote_user = bunker_uri.signer_public_key().unwrap();
			let remote_npub = remote_user.to_bech32().unwrap();

			match Nip46Signer::new(bunker_uri, app_keys, Duration::from_secs(120), None).await {
				Ok(signer) => {
					let keyring = Entry::new(&remote_npub, "nostr_secret").unwrap();
					let _ = keyring.set_password(&app_secret);

					// Update signer
					let _ = client.set_signer(Some(signer.into())).await;

					Ok(remote_npub)
				}
				Err(err) => Err(err.to_string()),
			}
		}
		Err(err) => Err(err.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn get_contact_list(state: State<'_, Nostr>) -> Result<Vec<String>, ()> {
	let contact_list = state.contact_list.lock().await;
	let list = contact_list.clone().into_iter().map(|c| c.public_key.to_hex()).collect::<Vec<_>>();

	Ok(list)
}

#[tauri::command]
#[specta::specta]
pub async fn login(
	id: String,
	bunker: Option<String>,
	state: State<'_, Nostr>,
	handle: tauri::AppHandle,
) -> Result<String, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let hex = public_key.to_hex();
	let keyring = Entry::new(&id, "nostr_secret").expect("Unexpected.");

	let password = match keyring.get_password() {
		Ok(pw) => pw,
		Err(_) => return Err("Cancelled".into()),
	};

	match bunker {
		Some(uri) => {
			let app_keys =
				Keys::parse(password).expect("Secret Key is modified, please check again.");

			match NostrConnectURI::parse(uri) {
				Ok(bunker_uri) => {
					match Nip46Signer::new(bunker_uri, app_keys, Duration::from_secs(30), None)
						.await
					{
						Ok(signer) => client.set_signer(Some(signer.into())).await,
						Err(err) => return Err(err.to_string()),
					}
				}
				Err(err) => return Err(err.to_string()),
			}
		}
		None => {
			let keys = Keys::parse(password).expect("Secret Key is modified, please check again.");
			let signer = NostrSigner::Keys(keys);

			// Update signer
			client.set_signer(Some(signer)).await;
		}
	}

	if let Ok(contacts) = client.get_contact_list(Some(Duration::from_secs(10))).await {
		let mut contact_list = state.contact_list.lock().await;
		*contact_list = contacts;
	};

	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	if let Ok(events) = client.get_events_of(vec![inbox], None).await {
		if let Some(event) = events.into_iter().next() {
			let urls = event
				.tags()
				.iter()
				.filter_map(|tag| {
					if let Some(TagStandard::Relay(relay)) = tag.as_standardized() {
						Some(relay.to_string())
					} else {
						None
					}
				})
				.collect::<Vec<_>>();

			for url in urls.iter() {
				let _ = client.add_relay(url).await;
				let _ = client.connect_relay(url).await;
			}

			let mut inbox_relays = state.inbox_relays.lock().await;
			inbox_relays.insert(public_key, urls);
		}
	}

	let subscription_id = SubscriptionId::new("personal_inbox");
	let new_message = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	if client.subscription(&subscription_id).await.is_some() {
		// Remove old subscriotion
		client.unsubscribe(subscription_id.clone()).await;
		// Resubscribe new message for current user
		let _ = client.subscribe_with_id(subscription_id, vec![new_message], None).await;
	} else {
		let _ = client.subscribe_with_id(subscription_id, vec![new_message], None).await;
	}

	let handle_clone = handle.app_handle().clone();

	tauri::async_runtime::spawn(async move {
		let window = handle_clone.get_webview_window("main").expect("Window is terminated.");
		let state = window.state::<Nostr>();
		let client = &state.client;

		let sync = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

		if client.reconcile(sync.clone(), NegentropyOptions::default()).await.is_ok() {
			handle_clone.emit("synchronized", ()).unwrap();
		};

		if client.get_events_of(vec![sync], Some(Duration::from_secs(20))).await.is_ok() {
			handle_clone.emit("synchronized", ()).unwrap();
		};
	});

	tauri::async_runtime::spawn(async move {
		let window = handle.get_webview_window("main").expect("Window is terminated.");
		let state = window.state::<Nostr>();
		let client = &state.client;

		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Event { event, .. } = notification {
					if event.kind == Kind::GiftWrap {
						if let Ok(UnwrappedGift { rumor, sender }) =
							client.unwrap_gift_wrap(&event).await
						{
							if let Err(e) = window.emit(
								"event",
								Payload { event: rumor.as_json(), sender: sender.to_hex() },
							) {
								println!("emit failed: {}", e)
							}
						}
					}
				}
				Ok(false)
			})
			.await
	});

	Ok(hex)
}
