use common::utils::room_hash;
use gpui::{AsyncApp, Context, Entity, Task};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use state::get_client;
use std::cmp::Reverse;

use crate::room::Room;

pub struct Inbox {
    pub rooms: Vec<Entity<Room>>,
    pub is_loading: bool,
}

impl Inbox {
    pub fn new() -> Self {
        Self {
            rooms: vec![],
            is_loading: true,
        }
    }

    pub fn get_room_ids(&self, cx: &Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }

    pub fn load(&mut self, cx: AsyncApp) -> Task<Result<Vec<Event>, Error>> {
        cx.background_executor().spawn(async move {
            let client = get_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key);

            // Get all DM events from database
            let events = client.database().query(filter).await?;

            // Filter result
            // - Get unique rooms only
            // - Sorted by created_at
            let result = events
                .into_iter()
                .filter(|ev| ev.tags.public_keys().peekable().peek().is_some())
                .unique_by(|ev| room_hash(&ev.tags))
                .sorted_by_key(|ev| Reverse(ev.created_at))
                .collect::<Vec<_>>();

            Ok(result)
        })
    }
}

impl Default for Inbox {
    fn default() -> Self {
        Self::new()
    }
}
