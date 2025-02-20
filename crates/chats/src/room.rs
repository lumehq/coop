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
    pub members: Vec<NostrProfile>,
    pub last_seen: LastSeen,
    // Store all new messages
    pub new_messages: Entity<Vec<Event>>,
}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        compare(&self.pubkeys(), &other.pubkeys())
    }
}

impl Room {
    pub fn new(
        id: u64,
        members: Vec<NostrProfile>,
        title: Option<SharedString>,
        last_seen: LastSeen,
        cx: &mut App,
    ) -> Self {
        let new_messages = cx.new(|_| Vec::new());

        Self {
            id,
            members,
            title,
            last_seen,
            new_messages,
        }
    }

    /// Convert nostr event to room
    pub fn parse(event: &Event, cx: &mut App) -> Room {
        let id = room_hash(event);
        let last_seen = LastSeen(event.created_at);
        let mut members: Vec<NostrProfile> = vec![];

        // Get all public keys from event's tags,
        // then convert them to NostrProfile
        members.extend(
            event
                .tags
                .public_keys()
                .collect::<HashSet<_>>()
                .into_iter()
                .map(|public_key| NostrProfile::new(*public_key, Metadata::default()))
                .collect::<Vec<_>>(),
        );

        // Convert event's pubkey to NostrProfile
        members.push(NostrProfile::new(event.pubkey, Metadata::default()));

        // Get title from event's tags,
        // and create random title if not found
        let title = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            Some(random_name(2).into())
        };

        Self::new(id, members, title, last_seen, cx)
    }

    /// Set contact's metadata by public key
    pub fn set_metadata(&mut self, public_key: PublicKey, metadata: Metadata) {
        for member in self.members.iter_mut() {
            if member.public_key() == public_key {
                member.set_metadata(&metadata);
            }
        }
    }

    /// Get room's member by public key
    pub fn member(&self, public_key: &PublicKey) -> Option<&NostrProfile> {
        self.members.iter().find(|m| &m.public_key() == public_key)
    }

    /// Get room's display name,
    /// this is combine all members' names
    pub fn name(&self) -> SharedString {
        if self.members.len() <= 2 {
            self.members
                .iter()
                .map(|profile| profile.name())
                .collect::<Vec<_>>()
                .join(", ")
                .into()
        } else {
            let name = self
                .members
                .iter()
                .take(2)
                .map(|profile| profile.name())
                .collect::<Vec<_>>()
                .join(", ");

            format!("{}, +{}", name, self.members.len() - 2).into()
        }
    }

    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    pub fn last_seen(&self) -> &LastSeen {
        &self.last_seen
    }

    /// Get all public keys from current room
    pub fn pubkeys(&self) -> Vec<PublicKey> {
        self.members.iter().map(|m| m.public_key()).collect()
    }
}
