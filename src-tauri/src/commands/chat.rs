use futures::stream::{self, StreamExt};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::{cmp::Reverse, time::Duration};
use tauri::State;

use crate::{common::is_member, Nostr};

#[tauri::command]
#[specta::specta]
pub async fn get_chats(db_only: bool, state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let signer = client.signer().await.map_err(|e| e.to_string())?;
	let public_key = signer.public_key().await.map_err(|e| e.to_string())?;

	let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

	let events = match db_only {
		true => match client.database().query(vec![filter], Order::Desc).await {
			Ok(events) => {
				stream::iter(events)
					.filter_map(|ev| async move {
						if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(&ev).await
						{
							if rumor.kind == Kind::PrivateDirectMessage {
								return Some(rumor);
							}
						}
						None
					})
					.collect::<Vec<_>>()
					.await
			}
			Err(err) => return Err(err.to_string()),
		},
		false => match client.get_events_of(vec![filter], Some(Duration::from_secs(12))).await {
			Ok(events) => {
				stream::iter(events)
					.filter_map(|ev| async move {
						if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(&ev).await
						{
							if rumor.kind == Kind::PrivateDirectMessage {
								return Some(rumor);
							}
						}
						None
					})
					.collect::<Vec<_>>()
					.await
			}
			Err(err) => return Err(err.to_string()),
		},
	};

	let uniqs = events
		.into_iter()
		.filter(|ev| ev.pubkey != public_key)
		.unique_by(|ev| ev.pubkey)
		.sorted_by_key(|ev| Reverse(ev.created_at))
		.map(|ev| ev.as_json())
		.collect::<Vec<_>>();

	Ok(uniqs)
}

#[tauri::command]
#[specta::specta]
pub async fn get_chat_messages(id: String, state: State<'_, Nostr>) -> Result<Vec<String>, String> {
	let client = &state.client;
	let signer = client.signer().await.map_err(|e| e.to_string())?;

	let receiver_pk = signer.public_key().await.map_err(|e| e.to_string())?;
	let sender_pk = PublicKey::parse(id).map_err(|e| e.to_string())?;

	let filter = Filter::new().kind(Kind::GiftWrap).pubkeys(vec![receiver_pk, sender_pk]);

	let rumors = match client.get_events_of(vec![filter], Some(Duration::from_secs(10))).await {
		Ok(events) => {
			stream::iter(events)
				.filter_map(|ev| async move {
					if let Ok(UnwrappedGift { rumor, sender }) = client.unwrap_gift_wrap(&ev).await
					{
						let groups = vec![&receiver_pk, &sender_pk];

						if groups.contains(&&sender) && is_member(groups, &rumor.tags) {
							Some(rumor.as_json())
						} else {
							None
						}
					} else {
						None
					}
				})
				.collect::<Vec<_>>()
				.await
		}
		Err(e) => return Err(e.to_string()),
	};

	Ok(rumors)
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
					if let Some(TagStandard::Relay(relay)) = tag.as_standardized() {
						let url = relay.to_string();
						let _ = client.add_relay(&url).await;
						let _ = client.connect_relay(&url).await;

						relays.push(url)
					}
				}

				let mut inbox_relays = state.inbox_relays.lock().await;
				inbox_relays.insert(public_key, relays.clone());
			}

			Ok(relays)
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
