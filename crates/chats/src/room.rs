use common::{
    last_seen::LastSeen,
    profile::NostrProfile,
    utils::{random_name, room_hash},
};
use gpui::{App, AppContext, Entity, SharedString};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::get_client;
use std::{collections::HashSet, rc::Rc};

pub struct Room {
    pub id: u64,
    pub last_seen: Rc<LastSeen>,
    /// Subject of the room (Nostr)
    pub title: String,
    /// Display name of the room (used for display purposes in Coop)
    pub display_name: Option<SharedString>,
    /// All members of the room
    pub members: SmallVec<[NostrProfile; 2]>,
    /// Store all new messages
    pub new_messages: Vec<Event>,
}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Room {
    pub fn new(event: &Event, cx: &mut App) -> Entity<Self> {
        let id = room_hash(event);
        let last_seen = Rc::new(LastSeen(event.created_at));
        // Get the subject from the event's tags, or create a random subject if none is found
        let title = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content()
                .map(|s| s.to_owned())
                .unwrap_or(random_name(2))
        } else {
            random_name(2)
        };

        let room = cx.new(|cx| {
            let this = Self {
                id,
                last_seen,
                title,
                display_name: None,
                members: smallvec![],
                new_messages: vec![],
            };

            let mut pubkeys = vec![];

            // Get all pubkeys from event's tags
            pubkeys.extend(event.tags.public_keys().collect::<HashSet<_>>());
            pubkeys.push(event.pubkey);

            let client = get_client();
            let (tx, rx) = oneshot::channel::<Vec<NostrProfile>>();

            cx.background_spawn(async move {
                let signer = client.signer().await.unwrap();
                let signer_pubkey = signer.get_public_key().await.unwrap();
                let mut profiles = vec![];

                for public_key in pubkeys.into_iter() {
                    if let Ok(result) = client.database().metadata(public_key).await {
                        let metadata = result.unwrap_or_default();
                        let profile = NostrProfile::new(public_key, metadata);

                        if public_key == signer_pubkey {
                            profiles.push(profile);
                        } else {
                            profiles.insert(0, profile);
                        }
                    }
                }

                _ = tx.send(profiles);
            })
            .detach();

            cx.spawn(|this, cx| async move {
                if let Ok(profiles) = rx.await {
                    _ = cx.update(|cx| {
                        let display_name = if profiles.len() > 2 {
                            let merged = profiles
                                .iter()
                                .take(2)
                                .map(|profile| profile.name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ");

                            let name: SharedString =
                                format!("{}, +{}", merged, profiles.len() - 2).into();

                            Some(name)
                        } else {
                            None
                        };

                        _ = this.update(cx, |this: &mut Room, cx| {
                            this.members.extend(profiles);
                            this.display_name = display_name;
                            cx.notify();
                        });
                    });
                }
            })
            .detach();

            this
        });

        room
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get room's member by public key
    pub fn member(&self, public_key: &PublicKey) -> Option<NostrProfile> {
        self.members
            .iter()
            .find(|m| &m.public_key() == public_key)
            .cloned()
    }

    /// Get room's first member's public key
    pub fn first_member(&self) -> Option<&NostrProfile> {
        self.members.first()
    }

    /// Collect room's member's public keys
    pub fn public_keys(&self) -> Vec<PublicKey> {
        self.members.iter().map(|m| m.public_key()).collect()
    }

    /// Get room's display name
    pub fn name(&self) -> Option<SharedString> {
        self.display_name.clone()
    }

    /// Determine if room is a group
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Get room's last seen
    pub fn last_seen(&self) -> Rc<LastSeen> {
        self.last_seen.clone()
    }

    /// Get room's last seen as ago format
    pub fn ago(&self) -> SharedString {
        self.last_seen.ago()
    }
}
