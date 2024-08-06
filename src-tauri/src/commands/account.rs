use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;
use serde::Serialize;
use std::{collections::HashSet, str::FromStr, time::Duration};
use tauri::{Emitter, Manager, State};

use crate::Nostr;

#[derive(Clone, Serialize)]
pub struct EventPayload {
	event: String, // JSON String
	sender: String,
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
pub async fn get_metadata(user_id: String, state: State<'_, Nostr>) -> Result<String, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&user_id).map_err(|e| e.to_string())?;
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
pub fn delete_account(id: String) -> Result<(), String> {
	let keyring = Entry::new("Coop Secret Storage", &id).map_err(|e| e.to_string())?;
	let _ = keyring.delete_credential();

	Ok(())
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
	let secret_key = keys.secret_key().map_err(|e| e.to_string())?;
	let enc = EncryptedSecretKey::new(secret_key, password, 16, KeySecurity::Medium)
		.map_err(|err| err.to_string())?;
	let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

	// Save account
	let keyring = Entry::new("Coop Secret Storage", &npub).unwrap();
	let _ = keyring.set_password(&enc_bech32);

	let signer = NostrSigner::Keys(keys);

	// Update signer
	client.set_signer(Some(signer)).await;

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
pub async fn import_key(
	key: String,
	password: Option<String>,
	state: State<'_, Nostr>,
) -> Result<String, String> {
	let client = &state.client;
	let secret_key = SecretKey::from_bech32(key).map_err(|err| err.to_string())?;
	let keys = Keys::new(secret_key.clone());
	let npub = keys.public_key().to_bech32().unwrap();

	let enc_bech32 = match password {
		Some(pw) => {
			let enc = EncryptedSecretKey::new(&secret_key, pw, 16, KeySecurity::Medium)
				.map_err(|err| err.to_string())?;

			enc.to_bech32().map_err(|err| err.to_string())?
		}
		None => secret_key.to_bech32().map_err(|err| err.to_string())?,
	};

	let keyring = Entry::new("Coop Secret Storage", &npub).unwrap();
	let _ = keyring.set_password(&enc_bech32);

	let signer = NostrSigner::Keys(keys);

	// Update client's signer
	client.set_signer(Some(signer)).await;

	Ok(npub)
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
					let keyring = Entry::new("Coop Secret Storage", &remote_npub).unwrap();
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

	let bech32 = match keyring.get_password() {
		Ok(pw) => pw,
		Err(_) => return Err("Action have been cancelled".into()),
	};

	let ncryptsec = EncryptedSecretKey::from_bech32(bech32).map_err(|e| e.to_string())?;
	let secret_key = ncryptsec.to_secret_key(password).map_err(|_| "Wrong password.")?;
	let keys = Keys::new(secret_key);
	let public_key = keys.public_key();
	let signer = NostrSigner::Keys(keys);

	// Update signer
	client.set_signer(Some(signer)).await;

	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	if let Ok(events) = client.get_events_of(vec![inbox], Some(Duration::from_secs(5))).await {
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
			.get_events_of_with_opts(
				vec![filter],
				Some(Duration::from_secs(20)),
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
				if let RelayPoolNotification::Event { event, subscription_id, .. } = notification {
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

							if let Err(e) = client.database().save_event(&ev).await {
								println!("Error: {}", e)
							}

							let payload =
								EventPayload { event: rumor.as_json(), sender: sender.to_hex() };

							handle.emit("event", payload).unwrap();
						}
					}
				}
				Ok(false)
			})
			.await
	});

	Ok(public_key.to_hex())
}
