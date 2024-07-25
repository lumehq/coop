use std::{cmp::Reverse, time::Duration};

use futures::stream::{self, StreamExt};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use tauri::State;

use crate::{common::is_target, Nostr};

#[tauri::command]
#[specta::specta]
pub async fn get_chats(state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let signer = client.signer().await.expect("Unexpected");
	let public_key = signer.public_key().await.expect("Unexpected");

	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	match client.database().query(vec![filter], Order::Desc).await {
		Ok(events) => {
			let rumors = stream::iter(events)
				.filter_map(|ev| async move {
					if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(&ev).await {
						if rumor.kind == Kind::PrivateDirectMessage {
							return Some(rumor);
						}
					}
					None
				})
				.collect::<Vec<_>>()
				.await;

			let uniqs = rumors
				.into_iter()
				.filter(|ev| ev.pubkey != public_key)
				.unique_by(|ev| ev.pubkey)
				.sorted_by_key(|ev| Reverse(ev.created_at))
				.map(|ev| ev.as_json())
				.collect::<Vec<_>>();

			Ok(uniqs)
		}
		Err(err) => Err(err.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn get_chat_messages(
	sender: String,
	state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
	let client = &state.client;
	let database = client.database();
	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let receiver_pk = signer.public_key().await.map_err(|e| e.to_string())?;
	let sender_pk = PublicKey::parse(sender).map_err(|e| e.to_string())?;

	let filter = Filter::new().kind(Kind::GiftWrap).pubkeys(vec![receiver_pk, sender_pk]);

	match database.query(vec![filter], Order::Desc).await {
		Ok(events) => {
			let rumors = stream::iter(events)
				.filter_map(|ev| async move {
					if let Ok(UnwrappedGift { rumor, sender }) = client.unwrap_gift_wrap(&ev).await
					{
						if rumor.kind == Kind::PrivateDirectMessage
							&& (sender == sender_pk || is_target(&sender_pk, &rumor.tags))
						{
							return Some(rumor);
						}
					}
					None
				})
				.map(|ev| ev.as_json())
				.collect::<Vec<_>>()
				.await;

			Ok(rumors)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn subscribe_to(id: String, state: State<'_, Nostr>) -> Result<(), String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;

	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key).limit(0);
	let subscription_id = SubscriptionId::new(&id[..6]);

	if client.subscribe_with_id(subscription_id, vec![filter], None).await.is_ok() {
		println!("Watching ... {}", id)
	};

	Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn unsubscribe(id: String, state: State<'_, Nostr>) -> Result<(), ()> {
	let client = &state.client;
	let subscription_id = SubscriptionId::new(&id[..6]);

	client.unsubscribe(subscription_id).await;
	println!("Unwatching ... {}", id);

	Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_inboxes(id: String, state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;
	let inbox = Filter::new().kind(Kind::Custom(10050)).author(public_key).limit(1);

	match client.get_events_of(vec![inbox], Some(Duration::from_secs(2))).await {
		Ok(events) => {
			let mut relays = Vec::new();

			if let Some(event) = events.into_iter().next() {
				for tag in &event.tags {
					if let Some(TagStandard::Relay(url)) = tag.as_standardized() {
						let relay = url.to_string();
						let _ = client.add_relay(&relay).await;
						let _ = client.connect_relay(&relay).await;

						relays.push(relay);
					}
				}
			}

			Ok(relays)
		}
		Err(e) => Err(e.to_string()),
	}
}

#[tauri::command]
#[specta::specta]
pub async fn drop_inbox(relays: Vec<String>, state: State<'_, Nostr>) -> Result<(), ()> {
	let client = &state.client;

	for relay in relays.iter() {
		let _ = client.disconnect_relay(relay).await;
	}

	Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn send_message(
	to: String,
	message: String,
	relays: Vec<String>,
	state: State<'_, Nostr>,
) -> Result<(), String> {
	let client = &state.client;
	let receiver = PublicKey::parse(&to).map_err(|e| e.to_string())?;

	match client.send_private_msg_to(relays, receiver, message, None).await {
		Ok(_) => Ok(()),
		Err(e) => Err(e.to_string()),
	}
}
