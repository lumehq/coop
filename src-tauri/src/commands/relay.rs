use nostr_sdk::prelude::*;
use std::{
	fs::OpenOptions,
	io::{self, BufRead, Write},
	time::Duration,
};
use tauri::{Manager, State};

use crate::Nostr;

async fn connect_nip65_relays(public_key: PublicKey, client: &Client) -> Vec<String> {
	let filter = Filter::new().author(public_key).kind(Kind::RelayList).limit(1);
	let mut relay_list: Vec<String> = Vec::new();

	if let Ok(events) =
		client.get_events_of(vec![filter], EventSource::relays(Some(Duration::from_secs(3)))).await
	{
		if let Some(event) = events.first() {
			for (url, ..) in nip65::extract_relay_list(event) {
				let _ = client.add_relay(url).await;
				relay_list.push(url.to_string())
			}
		}
	};

	relay_list
}

async fn disconnect_nip65_relays(relays: Vec<String>, client: &Client) {
	for relay in relays.iter() {
		if let Err(e) = client.disconnect_relay(relay).await {
			println!("Disconnect failed: {}", e)
		}
	}
}

#[tauri::command]
#[specta::specta]
pub fn get_bootstrap_relays(app: tauri::AppHandle) -> Result<Vec<String>, String> {
	let relays_path = app
		.path()
		.resolve("resources/relays.txt", tauri::path::BaseDirectory::Resource)
		.map_err(|e| e.to_string())?;

	let file = std::fs::File::open(relays_path).map_err(|e| e.to_string())?;
	let reader = io::BufReader::new(file);

	reader.lines().collect::<Result<Vec<String>, io::Error>>().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_bootstrap_relays(relays: String, app: tauri::AppHandle) -> Result<(), String> {
	let relays_path = app
		.path()
		.resolve("resources/relays.txt", tauri::path::BaseDirectory::Resource)
		.map_err(|e| e.to_string())?;
	let mut file = OpenOptions::new().write(true).open(relays_path).map_err(|e| e.to_string())?;

	file.write_all(relays.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn collect_inbox_relays(
	user_id: String,
	state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(user_id).map_err(|e| e.to_string())?;
	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	match client.get_events_of(vec![inbox], EventSource::relays(Some(Duration::from_secs(3)))).await
	{
		Ok(events) => {
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

				Ok(urls)
			} else {
				Ok(Vec::new())
			}
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn set_inbox_relays(relays: Vec<String>, state: State<'_, Nostr>) -> Result<(), String> {
	let client = &state.client;

	let tags = relays.into_iter().map(|t| Tag::custom(TagKind::Relay, vec![t])).collect::<Vec<_>>();
	let event = EventBuilder::new(Kind::Custom(10050), "", tags);

	match client.send_event_builder(event).await {
		Ok(_) => Ok(()),
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn connect_inbox_relays(
	user_id: String,
	ignore_cache: bool,
	state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&user_id).map_err(|e| e.to_string())?;

	// let nip65_relays = connect_nip65_relays(public_key, client).await;
	let mut inbox_relays = state.inbox_relays.lock().await;

	if !ignore_cache {
		if let Some(relays) = inbox_relays.get(&public_key) {
			for url in relays {
				if let Ok(relay) = client.relay(url).await {
					if !relay.is_connected().await {
						if let Err(e) = client.connect_relay(url).await {
							println!("Connect relay failed: {}", e)
						}
					}
				} else if let Err(e) = client.add_relay(url).await {
					println!("Connect relay failed: {}", e)
				}
			}
			return Ok(relays.to_owned());
		};
	};

	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	match client.get_events_of(vec![inbox], EventSource::relays(Some(Duration::from_secs(3)))).await
	{
		Ok(events) => {
			let mut relays = Vec::new();

			if let Some(event) = events.into_iter().next() {
				for tag in &event.tags {
					if let Some(TagStandard::Relay(relay)) = tag.as_standardized() {
						let url = relay.to_string();

						if let Err(e) = client.add_relay(&url).await {
							println!("Connect relay failed: {}", e)
						};

						relays.push(url)
					}
				}

				// Update state
				inbox_relays.insert(public_key, relays.clone());

				// Disconnect user's nip65 relays to save bandwidth
				// disconnect_nip65_relays(nip65_relays, client).await;
			}

			Ok(relays)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect_inbox_relays(
	user_id: String,
	state: State<'_, Nostr>,
) -> Result<(), String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&user_id).map_err(|e| e.to_string())?;
	let inbox_relays = state.inbox_relays.lock().await;

	if let Some(relays) = inbox_relays.get(&public_key) {
		for relay in relays {
			let _ = client.disconnect_relay(relay).await;
		}
	}

	Ok(())
}
