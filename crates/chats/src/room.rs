use common::{
    last_seen::LastSeen,
    profile::NostrProfile,
    utils::{compare, random_name, room_hash},
};
use gpui::{App, AppContext, Entity, SharedString};
use nostr_sdk::prelude::*;
use std::collections::HashSet;

pub struct Room {
    pub id: u64,
    pub title: Option<SharedString>,
    pub owner: NostrProfile,        // Owner always match current user
    pub members: Vec<NostrProfile>, // Extract from event's tags
    pub last_seen: LastSeen,
    pub is_group: bool,
    pub new_messages: Entity<Vec<Event>>, // Hold all new messages
}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        compare(&self.pubkeys(), &other.pubkeys())
    }
}

impl Room {
    pub fn new(
        id: u64,
        owner: NostrProfile,
        members: Vec<NostrProfile>,
        title: Option<SharedString>,
        last_seen: LastSeen,
        cx: &mut App,
    ) -> Self {
        let new_messages = cx.new(|_| Vec::new());
        let is_group = members.len() > 1;
        let title = if title.is_none() {
            Some(random_name(2).into())
        } else {
            title
        };

        Self {
            id,
            owner,
            members,
            title,
            last_seen,
            is_group,
            new_messages,
        }
    }

    /// Convert nostr event to room
    pub fn parse(event: &Event, cx: &mut App) -> Room {
        let id = room_hash(event);
        let last_seen = LastSeen(event.created_at);

        // Always equal to current user
        let owner = NostrProfile::new(event.pubkey, Metadata::default());

        // Get all pubkeys that invole in this group
        let members: Vec<NostrProfile> = event
            .tags
            .public_keys()
            .collect::<HashSet<_>>()
            .into_iter()
            .map(|public_key| NostrProfile::new(*public_key, Metadata::default()))
            .collect();

        // Get title from event's tags
        let title = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            None
        };

        Self::new(id, owner, members, title, last_seen, cx)
    }

    /// Set contact's metadata by public key
    pub fn set_metadata(&mut self, public_key: PublicKey, metadata: Metadata) {
        if self.owner.public_key() == public_key {
            self.owner.set_metadata(&metadata);
        }

        for member in self.members.iter_mut() {
            if member.public_key() == public_key {
                member.set_metadata(&metadata);
            }
        }
    }

    /// Get room's member by public key
    pub fn member(&self, public_key: &PublicKey) -> Option<NostrProfile> {
        if &self.owner.public_key() == public_key {
            Some(self.owner.clone())
        } else {
            self.members
                .iter()
                .find(|m| &m.public_key() == public_key)
                .cloned()
        }
    }

    /// Get room's display name
    pub fn name(&self) -> String {
        if self.members.len() <= 2 {
            self.members
                .iter()
                .map(|profile| profile.name())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            let name = self
                .members
                .iter()
                .take(2)
                .map(|profile| profile.name())
                .collect::<Vec<_>>()
                .join(", ");

            format!("{}, +{}", name, self.members.len() - 2)
        }
    }

    pub fn last_seen(&self) -> &LastSeen {
        &self.last_seen
    }

    /// Get all public keys from current room
    pub fn pubkeys(&self) -> Vec<PublicKey> {
        let mut pubkeys: Vec<_> = self.members.iter().map(|m| m.public_key()).collect();
        pubkeys.push(self.owner.public_key());

        pubkeys
    }
}
