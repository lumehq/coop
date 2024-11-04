use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{collections::HashSet, str::FromStr, time::Duration};
use tauri::{Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use crate::Nostr;

#[derive(Clone, Serialize)]
pub struct EventPayload {
	event: String, // JSON String
	sender: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
struct Account {
	password: String,
	nostr_connect: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_metadata(id: String, state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;

	let filter = Filter::new().author(public_key).kind(Kind::Metadata).limit(1);

	let events = client.database().query(vec![filter]).await.map_err(|e| e.to_string())?;

	match events.first() {
		Some(event) => match Metadata::from_json(&event.content) {
			Ok(metadata) => Ok(metadata.as_json()),
			Err(e) => Err(e.to_string()),
		},
		None => Err("Metadata not found".into()),
	}
}

#[tauri::command]
#[specta::specta]
pub fn get_accounts() -> Vec<String> {
	let search = Search::new().expect("Unexpected.");
	let results = search.by_service("Coop Secret Storage");
	let list = List::list_credentials(&results, Limit::All);
	let accounts: HashSet<String> =
		list.split_whitespace().filter(|v| v.starts_with("npub1")).map(String::from).collect();

	accounts.into_iter().collect()
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_account(state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;
	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let public_key = signer.get_public_key().await.map_err(|e| e.to_string())?;
	let bech32 = public_key.to_bech32().map_err(|e| e.to_string())?;

	Ok(bech32)
}

#[tauri::command]
#[specta::specta]
pub async fn create_account(
	name: String,
	about: String,
	picture: String,
	password: String,
	state: State<'_, Nostr>,
) -> Result<String, String> {
	let client = &state.client;
	let keys = Keys::generate();
	let npub = keys.public_key().to_bech32().map_err(|e| e.to_string())?;
	let secret_key = keys.secret_key();
	let enc = EncryptedSecretKey::new(secret_key, password, 16, KeySecurity::Medium)
		.map_err(|err| err.to_string())?;
	let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

	// Save account
	let keyring = Entry::new("Coop Secret Storage", &npub).map_err(|e| e.to_string())?;
	let account = Account { password: enc_bech32, nostr_connect: None };
	let j = serde_json::to_string(&account).map_err(|e| e.to_string())?;
	let _ = keyring.set_password(&j);

	// Update signer
	client.set_signer(keys).await;

	let mut metadata =
		Metadata::new().display_name(name.clone()).name(name.to_lowercase()).about(about);

	if let Ok(url) = Url::parse(&picture) {
		metadata = metadata.picture(url)
	}

	match client.set_metadata(&metadata).await {
		Ok(_) => Ok(npub),
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn import_account(key: String, password: String) -> Result<String, String> {
	let (npub, enc_bech32) = match key.starts_with("ncryptsec") {
		true => {
			let enc = EncryptedSecretKey::from_bech32(key).map_err(|err| err.to_string())?;
			let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;
			let secret_key = enc.to_secret_key(password).map_err(|err| err.to_string())?;
			let keys = Keys::new(secret_key);
			let npub = keys.public_key().to_bech32().unwrap();

			(npub, enc_bech32)
		}
		false => {
			let secret_key = SecretKey::from_bech32(key).map_err(|err| err.to_string())?;
			let keys = Keys::new(secret_key.clone());
			let npub = keys.public_key().to_bech32().unwrap();

			let enc = EncryptedSecretKey::new(&secret_key, password, 16, KeySecurity::Medium)
				.map_err(|err| err.to_string())?;

			let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

			(npub, enc_bech32)
		}
	};

	let keyring = Entry::new("Coop Secret Storage", &npub).map_err(|e| e.to_string())?;

	let account = Account { password: enc_bech32, nostr_connect: None };

	let pwd = serde_json::to_string(&account).map_err(|e| e.to_string())?;
	keyring.set_password(&pwd).map_err(|e| e.to_string())?;

	Ok(npub)
}

#[tauri::command]
#[specta::specta]
pub async fn connect_account(uri: String, state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;

	match NostrConnectURI::parse(uri.clone()) {
		Ok(bunker_uri) => {
			// Local user
			let app_keys = Keys::generate();
			let app_secret = app_keys.secret_key().to_secret_hex();

			// Get remote user
			let remote_user = bunker_uri.remote_signer_public_key().unwrap();
			let remote_npub = remote_user.to_bech32().unwrap();

			match NostrConnect::new(bunker_uri, app_keys, Duration::from_secs(120), None) {
				Ok(signer) => {
					let mut url = Url::parse(&uri).unwrap();
					let query: Vec<(String, String)> = url
						.query_pairs()
						.filter(|(name, _)| name != "secret")
						.map(|(name, value)| (name.into_owned(), value.into_owned()))
						.collect();
					url.query_pairs_mut().clear().extend_pairs(&query);

					let key = format!("{}_nostrconnect", remote_npub);
					let keyring = Entry::new("Coop Secret Storage", &key).unwrap();
					let account =
						Account { password: app_secret, nostr_connect: Some(url.to_string()) };
					let j = serde_json::to_string(&account).map_err(|e| e.to_string())?;
					let _ = keyring.set_password(&j);

					// Update signer
					let _ = client.set_signer(signer).await;

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
pub async fn reset_password(key: String, password: String) -> Result<(), String> {
	let secret_key = SecretKey::from_bech32(key).map_err(|err| err.to_string())?;
	let keys = Keys::new(secret_key.clone());
	let npub = keys.public_key().to_bech32().unwrap();

	let enc = EncryptedSecretKey::new(&secret_key, password, 16, KeySecurity::Medium)
		.map_err(|err| err.to_string())?;
	let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

	let keyring = Entry::new("Coop Secret Storage", &npub).map_err(|e| e.to_string())?;
	let account = Account { password: enc_bech32, nostr_connect: None };
	let j = serde_json::to_string(&account).map_err(|e| e.to_string())?;
	let _ = keyring.set_password(&j);

	Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn delete_account(id: String) -> Result<(), String> {
	let keyring = Entry::new("Coop Secret Storage", &id).map_err(|e| e.to_string())?;
	let _ = keyring.delete_credential();

	Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_contact_list(state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;

	match client.get_contact_list(Some(Duration::from_secs(10))).await {
		Ok(contacts) => {
			let list = contacts.into_iter().map(|c| c.public_key.to_hex()).collect::<Vec<_>>();
			Ok(list)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn login(
	account: String,
	password: String,
	state: State<'_, Nostr>,
	handle: tauri::AppHandle,
) -> Result<String, String> {
	let client = &state.client;
	let keyring = Entry::new("Coop Secret Storage", &account).map_err(|e| e.to_string())?;

	let account = match keyring.get_password() {
		Ok(pw) => {
			let account: Account = serde_json::from_str(&pw).map_err(|e| e.to_string())?;
			account
		}
		Err(e) => return Err(e.to_string()),
	};

	let public_key = match account.nostr_connect {
		None => {
			let ncryptsec =
				EncryptedSecretKey::from_bech32(account.password).map_err(|e| e.to_string())?;
			let secret_key = ncryptsec.to_secret_key(password).map_err(|_| "Wrong password.")?;
			let keys = Keys::new(secret_key);
			let public_key = keys.public_key();

			// Update signer
			client.set_signer(keys).await;

			public_key
		}
		Some(bunker) => {
			let uri = NostrConnectURI::parse(bunker).map_err(|e| e.to_string())?;
			let public_key = uri.remote_signer_public_key().unwrap().clone();
			let app_keys = Keys::from_str(&account.password).map_err(|e| e.to_string())?;

			match NostrConnect::new(uri, app_keys, Duration::from_secs(120), None) {
				Ok(signer) => {
					// Update signer
					client.set_signer(signer).await;
					// Return public key
					public_key
				}
				Err(e) => return Err(e.to_string()),
			}
		}
	};

	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	if let Ok(events) =
		client.get_events_of(vec![inbox], EventSource::relays(Some(Duration::from_secs(3)))).await
	{
		if let Some(event) = events.into_iter().next() {
			let urls = event
				.tags
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
				if let Err(e) = client.add_relay(url).await {
					println!("Connect relay failed: {}", e)
				}
			}

			// Workaround for https://github.com/rust-nostr/nostr/issues/509
			// TODO: remove this
			let _ = client
				.get_events_from(
					urls.clone(),
					vec![Filter::new().kind(Kind::TextNote).limit(0)],
					Some(Duration::from_secs(5)),
				)
				.await;

			let mut inbox_relays = state.inbox_relays.lock().await;
			inbox_relays.insert(public_key, urls);
		} else {
			return Err("404".into());
		}
	}

	let sub_id = SubscriptionId::new("inbox");
	let new_message = Filter::new().kind(Kind::GiftWrap).pubkey(public_key).limit(0);

	if client.subscription(&sub_id).await.is_some() {
		// Remove old subscriotion
		client.unsubscribe(sub_id.clone()).await;
		// Resubscribe new message for current user
		let _ = client.subscribe_with_id(sub_id.clone(), vec![new_message], None).await;
	} else {
		let _ = client.subscribe_with_id(sub_id.clone(), vec![new_message], None).await;
	}

	tauri::async_runtime::spawn(async move {
		let state = handle.state::<Nostr>();
		let client = &state.client;

		let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

		// Generate a fake sig for rumor event.
		// TODO: Find better way to save unsigned event to database.
		let fake_sig = Signature::from_str("f9e79d141c004977192d05a86f81ec7c585179c371f7350a5412d33575a2a356433f58e405c2296ed273e2fe0aafa25b641e39cc4e1f3f261ebf55bce0cbac83").unwrap();

		if let Ok(events) = client
			.pool()
			.get_events_of(
				vec![filter],
				Duration::from_secs(12),
				FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(20)),
			)
			.await
		{
			for event in events.iter() {
				if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(event).await {
					let rumor_clone = rumor.clone();
					let ev = Event::new(
						rumor_clone.id.unwrap(),
						rumor_clone.pubkey,
						rumor_clone.created_at,
						rumor_clone.kind,
						rumor_clone.tags,
						rumor_clone.content,
						fake_sig,
					);

					if let Err(e) = client.database().save_event(&ev).await {
						println!("Error: {}", e)
					}
				}
			}
			handle.emit("synchronized", ()).unwrap();
		}

		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Message { message, .. } = notification {
					if let RelayMessage::Event { event, subscription_id, .. } = message {
						if subscription_id == sub_id && event.kind == Kind::GiftWrap {
							if let Ok(UnwrappedGift { rumor, sender }) =
								client.unwrap_gift_wrap(&event).await
							{
								let rumor_clone = rumor.clone();
								let ev = Event::new(
									rumor_clone.id.unwrap(),
									rumor_clone.pubkey,
									rumor_clone.created_at,
									rumor_clone.kind,
									rumor_clone.tags,
									rumor_clone.content,
									fake_sig,
								);

								// Save rumor to database to further query
								if let Err(e) = client.database().save_event(&ev).await {
									println!("[save event] error: {}", e)
								}

								// Emit new event to frontend
								if let Err(e) = handle.emit(
									"event",
									EventPayload {
										event: rumor.as_json(),
										sender: sender.to_hex(),
									},
								) {
									println!("[emit] error: {}", e)
								}

								if sender != public_key {
									if let Some(window) = handle.get_webview_window("main") {
										if !window.is_focused().unwrap() {
											if let Err(e) = handle
												.notification()
												.builder()
												.body("You have a new message")
												.title("Coop")
												.show()
											{
												println!("[notification] error: {}", e);
											}
										}
									}
								}
							}
						}
					} else {
						println!("relay message: {}", message.as_json())
					}
				}
				Ok(false)
			})
			.await
	});

	Ok(public_key.to_hex())
}
