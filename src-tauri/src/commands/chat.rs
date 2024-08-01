use nostr_sdk::prelude::*;
use serde::Serialize;
use std::time::Duration;
use tauri::{Emitter, Manager, State};

use crate::{
	common::{process_chat_event, process_message_event},
	Nostr,
};

#[derive(Clone, Serialize)]
pub struct ChatPayload {
	events: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_chats(
	state: State<'_, Nostr>,
	handle: tauri::AppHandle,
) -> Result<Vec<String>, String> {
	let client = &state.client;
	let database = client.database();
	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let public_key = signer.public_key().await.map_err(|e| e.to_string())?;

	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	let events = match database.query(vec![filter.clone()], Order::Desc).await {
		Ok(events) => process_chat_event(client, events).await,
		Err(e) => return Err(e.to_string()),
	};

	tauri::async_runtime::spawn(async move {
		let state = handle.state::<Nostr>();
		let client = &state.client;

		if let Ok(events) = client.get_events_of(vec![filter], None).await {
			let rumors = process_chat_event(client, events).await;
			handle.emit("sync_chat", ChatPayload { events: rumors }).unwrap();
		}
	});

	Ok(events)
}

#[tauri::command]
#[specta::specta]
pub async fn get_chat_messages(
	id: String,
	state: State<'_, Nostr>,
	handle: tauri::AppHandle,
) -> Result<Vec<String>, String> {
	let client = &state.client;
	let database = client.database();

	let signer = client.signer().await.map_err(|e| e.to_string())?;

	let public_key = signer.public_key().await.map_err(|e| e.to_string())?;
	let sender = PublicKey::parse(id.clone()).map_err(|e| e.to_string())?;

	let group = vec![public_key, sender];
	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	let rumors = match database.query(vec![filter.clone()], Order::Desc).await {
		Ok(events) => process_message_event(client, events, &group).await,
		Err(e) => return Err(e.to_string()),
	};

	tauri::async_runtime::spawn(async move {
		let state = handle.state::<Nostr>();
		let client = &state.client;

		if let Ok(events) = client.get_events_of(vec![filter], None).await {
			let rumors = process_message_event(client, events, &group).await;
			let emit_to = format!("sync_chat_{}", id);

			handle.emit(&emit_to, ChatPayload { events: rumors }).unwrap();
		}
	});

	Ok(rumors)
}

#[tauri::command]
#[specta::specta]
pub async fn connect_inbox(id: String, state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let mut inbox_relays = state.inbox_relays.lock().await;

	if let Some(relays) = inbox_relays.get(&public_key) {
		for relay in relays {
			let _ = client.connect_relay(relay).await;
		}
		return Ok(relays.to_owned());
	}

	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	match client.get_events_of(vec![inbox], Some(Duration::from_secs(2))).await {
		Ok(events) => {
			let mut relays = Vec::new();

			if let Some(event) = events.into_iter().next() {
				for tag in &event.tags {
					if let Some(TagStandard::Relay(relay)) = tag.as_standardized() {
						let url = relay.to_string();
						let _ = client.add_relay(&url).await;
						let _ = client.connect_relay(&url).await;

						relays.push(url)
					}
				}

				inbox_relays.insert(public_key, relays.clone());
			}

			Ok(relays)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect_inbox(id: String, state: State<'_, Nostr>) -> Result<(), String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let inbox_relays = state.inbox_relays.lock().await;

	if let Some(relays) = inbox_relays.get(&public_key) {
		for relay in relays {
			let _ = client.disconnect_relay(relay).await;
		}
	}

	Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn send_message(
	to: String,
	message: String,
	state: State<'_, Nostr>,
) -> Result<(), String> {
	let client = &state.client;

	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let public_key = signer.public_key().await.map_err(|e| e.to_string())?;
	let receiver = PublicKey::parse(&to).map_err(|e| e.to_string())?;

	// TODO: Add support reply_to
	let rumor = EventBuilder::private_msg_rumor(receiver, message, None);

	// Get inbox relays
	let relays = state.inbox_relays.lock().await;

	let outbox = relays.get(&receiver);
	let inbox = relays.get(&public_key);

	let outbox_urls = match outbox {
		Some(relays) => relays,
		None => return Err("User's didn't have inbox relays to receive message.".into()),
	};

	let inbox_urls = match inbox {
		Some(relays) => relays,
		None => return Err("User's didn't have inbox relays to receive message.".into()),
	};

	match client.gift_wrap_to(outbox_urls, receiver, rumor.clone(), None).await {
		Ok(_) => {
			if let Err(e) = client.gift_wrap_to(inbox_urls, public_key, rumor, None).await {
				return Err(e.to_string());
			}

			Ok(())
		}
		Err(e) => Err(e.to_string()),
	}
}
