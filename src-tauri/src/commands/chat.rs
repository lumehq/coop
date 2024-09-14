use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::cmp::Reverse;
use tauri::State;

use crate::Nostr;

#[tauri::command]
#[specta::specta]
pub async fn get_chats(state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let public_key = signer.public_key().await.map_err(|e| e.to_string())?;

	let filter = Filter::new().kind(Kind::PrivateDirectMessage).pubkey(public_key);

	match client.database().query(vec![filter]).await {
		Ok(events) => {
			let ev = events
				.into_iter()
				.sorted_by_key(|ev| Reverse(ev.created_at))
				.filter(|ev| ev.pubkey != public_key)
				.unique_by(|ev| ev.pubkey)
				.map(|ev| ev.as_json())
				.collect::<Vec<_>>();

			Ok(ev)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn get_chat_messages(id: String, state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let signer = client.signer().await.map_err(|e| e.to_string())?;

	let receiver = signer.public_key().await.map_err(|e| e.to_string())?;
	let sender = PublicKey::parse(id).map_err(|e| e.to_string())?;

	let recv_filter =
		Filter::new().kind(Kind::PrivateDirectMessage).author(sender).pubkey(receiver);
	let sender_filter =
		Filter::new().kind(Kind::PrivateDirectMessage).author(receiver).pubkey(sender);

	match client.database().query(vec![recv_filter, sender_filter]).await {
		Ok(events) => {
			let ev = events.into_iter().map(|ev| ev.as_json()).collect::<Vec<_>>();
			Ok(ev)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn send_message(
	to: String,
	message: String,
	state: State<'_, Nostr>,
) -> Result<(), String> {
	let client = &state.client;
	let relays = state.inbox_relays.lock().await;

	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let public_key = signer.public_key().await.map_err(|e| e.to_string())?;
	let receiver = PublicKey::parse(&to).map_err(|e| e.to_string())?;

	// TODO: Add support reply_to
	let rumor = EventBuilder::private_msg_rumor(receiver, message, None);

	// Get inbox relays per member
	let outbox_urls = match relays.get(&receiver) {
		Some(relays) => relays,
		None => return Err("Receiver didn't have inbox relays to receive message.".into()),
	};

	let inbox_urls = match relays.get(&public_key) {
		Some(relays) => relays,
		None => return Err("Please config inbox relays to backup your message.".into()),
	};

	// Send message to [receiver]
	match client.gift_wrap_to(outbox_urls, &receiver, rumor.clone(), None).await {
		Ok(_) => {
			// Send message to [yourself]
			if let Err(e) = client.gift_wrap_to(inbox_urls, &public_key, rumor, None).await {
				return Err(e.to_string());
			}

			Ok(())
		}
		Err(e) => Err(e.to_string()),
	}
}
