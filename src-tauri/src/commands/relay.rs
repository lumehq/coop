use nostr_sdk::prelude::*;
use std::{
	fs::OpenOptions,
	io::{self, BufRead, Write},
};
use tauri::{Manager, State};

use crate::Nostr;

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
pub async fn get_inbox_relays(
	user_id: String,
	state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(user_id).map_err(|e| e.to_string())?;
	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	match client.get_events_of(vec![inbox], None).await {
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
