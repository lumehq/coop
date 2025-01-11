use crate::utils::{room_hash, shorted_public_key};
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

    pub fn metadata(&self) -> Metadata {
        self.metadata.clone()
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
    pub owner: PublicKey,
    pub members: Vec<Member>,
    pub last_seen: Timestamp,
    pub is_group: bool,
}

impl Room {
    pub fn new(event: &Event) -> Self {
        let id = room_hash(&event.tags);
        let last_seen = event.created_at;

        let owner = event.pubkey;
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
        }
    }

    pub fn set_metadata(&mut self, public_key: PublicKey, metadata: Metadata) {
        for member in self.members.iter_mut() {
            if member.public_key() == public_key {
                member.update(&metadata);
            }
        }
    }
}
