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
				.unique_by(|ev| ev.pubkey)
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
