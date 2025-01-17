use crate::{
    constants::IMAGE_SERVICE,
    utils::{room_hash, shorted_public_key},
};
use gpui::SharedString;
use nostr_sdk::prelude::*;
use rnglib::{Language, RNG};

#[derive(Debug, Clone)]
pub struct Member {
    public_key: PublicKey,
    metadata: Metadata,
}

impl Member {
    pub fn new(public_key: PublicKey, metadata: Metadata) -> Self {
        Self {
            public_key,
            metadata,
        }
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn avatar(&self) -> String {
        if let Some(picture) = &self.metadata.picture {
            format!(
                "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1",
                IMAGE_SERVICE, picture
            )
        } else {
            "brand/avatar.png".into()
        }
    }

    pub fn name(&self) -> String {
        if let Some(display_name) = &self.metadata.display_name {
            if !display_name.is_empty() {
                return display_name.clone();
            }
        }

        if let Some(name) = &self.metadata.name {
            if !name.is_empty() {
                return name.clone();
            }
        }

        shorted_public_key(self.public_key)
    }

    pub fn update(&mut self, metadata: &Metadata) {
        self.metadata = metadata.clone()
    }
}

#[derive(Debug)]
pub struct Room {
    pub id: u64,
    pub title: Option<SharedString>,
    pub owner: Member,        // Owner always match current user
    pub members: Vec<Member>, // Extract from event's tags
    pub last_seen: Timestamp,
    pub is_group: bool,
    pub new_messages: Vec<Event>, // Hold all new messages
}

impl Room {
    pub fn new(event: &Event) -> Self {
        let id = room_hash(&event.tags);
        let last_seen = event.created_at;

        let owner = Member::new(event.pubkey, Metadata::default());
        let members: Vec<Member> = event
            .tags
            .public_keys()
            .copied()
            .map(|public_key| Member::new(public_key, Metadata::default()))
            .collect();

        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            let rng = RNG::from(&Language::Roman);
            let name = rng.generate_names(2, true).join("-").to_lowercase();

            Some(name.into())
        };

        let is_group = members.len() > 1;

        Self {
            id,
            owner,
            members,
            title,
            last_seen,
            is_group,
            new_messages: vec![],
        }
    }

    pub fn set_metadata(&mut self, public_key: PublicKey, metadata: Metadata) {
        if self.owner.public_key() == public_key {
            self.owner.update(&metadata);
        }

        for member in self.members.iter_mut() {
            if member.public_key() == public_key {
                member.update(&metadata);
            }
        }
    }

    pub fn member(&self, public_key: &PublicKey) -> Option<Member> {
        if &self.owner.public_key() == public_key {
            Some(self.owner.clone())
        } else {
            self.members
                .iter()
                .find(|m| &m.public_key() == public_key)
                .cloned()
        }
    }

    pub fn name(&self) -> String {
        self.members
            .iter()
            .map(|profile| profile.name())
            .collect::<Vec<String>>()
            .join(", ")
    }

    pub fn get_all_keys(&self) -> Vec<PublicKey> {
        let mut pubkeys: Vec<_> = self.members.iter().map(|m| m.public_key()).collect();
        pubkeys.push(self.owner.public_key());

        pubkeys
    }
}
