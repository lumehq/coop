use std::cmp::Reverse;

use futures::stream::{self, StreamExt};
use itertools::Itertools;
use nostr_sdk::prelude::*;

pub async fn process_chat_event(client: &Client, events: Vec<Event>) -> Vec<String> {
	let rumors = stream::iter(events)
		.filter_map(|ev| async move {
			if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(&ev).await {
				if rumor.kind == Kind::PrivateDirectMessage {
					Some(rumor)
				} else {
					None
				}
			} else {
				None
			}
		})
		.collect::<Vec<_>>()
		.await;

	let signer = client.signer().await.unwrap();
	let public_key = signer.public_key().await.unwrap();

	rumors
		.into_iter()
		.sorted_by_key(|ev| Reverse(ev.created_at))
		.filter(|ev| ev.pubkey != public_key)
		.unique_by(|ev| ev.pubkey)
		.map(|ev| ev.as_json())
		.collect::<Vec<_>>()
}

pub async fn process_message_event(
	client: &Client,
	events: Vec<Event>,
	group: &Vec<PublicKey>,
) -> Vec<String> {
	stream::iter(events)
		.filter_map(|ev| async move {
			if let Ok(UnwrappedGift { rumor, sender }) = client.unwrap_gift_wrap(&ev).await {
				if group.contains(&sender) && is_member(group, &rumor.tags) {
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

pub fn is_member(group: &Vec<PublicKey>, tags: &Vec<Tag>) -> bool {
	for tag in tags {
		if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
			if group.contains(public_key) {
				return true;
			}
		}
	}
	false
}
