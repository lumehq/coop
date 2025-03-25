use std::collections::HashSet;

use anyhow::Error;
use common::{last_seen::LastSeen, profile::NostrProfile, utils::room_hash};
use global::get_client;
use gpui::{App, AppContext, Entity, EventEmitter, SharedString, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};

#[derive(Debug, Clone)]
pub struct IncomingEvent {
    pub event: Event,
}

pub struct Room {
    pub id: u64,
    pub last_seen: LastSeen,
    /// Subject of the room
    pub name: Option<SharedString>,
    /// All members of the room
    pub members: SmallVec<[NostrProfile; 2]>,
}

impl EventEmitter<IncomingEvent> for Room {}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Room {
    pub fn new(event: &Event, cx: &mut App) -> Entity<Self> {
        let id = room_hash(event);
        let last_seen = LastSeen(event.created_at);

        // Get the subject from the event's tags
        let name = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            None
        };

        // Create a task for loading metadata
        let load_metadata = Self::load_metadata(event, cx);

        let room = cx.new(|cx| {
            let this = Self {
                id,
                last_seen,
                name,
                members: smallvec![],
            };

            cx.spawn(|this, cx| async move {
                if let Ok(profiles) = load_metadata.await {
                    _ = cx.update(|cx| {
                        _ = this.update(cx, |this: &mut Room, cx| {
                            // Update the room's name if it's not already set
                            if this.name.is_none() {
                                let mut name = profiles
                                    .iter()
                                    .take(2)
                                    .map(|profile| profile.name.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ");

                                if profiles.len() > 2 {
                                    name = format!("{}, +{}", name, profiles.len() - 2);
                                }

                                this.name = Some(name.into())
                            };
                            // Update the room's members
                            this.members.extend(profiles);

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
            .find(|m| &m.public_key == public_key)
            .cloned()
    }

    /// Get room's first member's public key
    pub fn first_member(&self) -> Option<&NostrProfile> {
        self.members.first()
    }

    /// Collect room's member's public keys
    pub fn public_keys(&self) -> Vec<PublicKey> {
        self.members.iter().map(|m| m.public_key).collect()
    }

    /// Get room's display name
    pub fn name(&self) -> Option<SharedString> {
        self.name.clone()
    }

    /// Determine if room is a group
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Get room's last seen
    pub fn last_seen(&self) -> LastSeen {
        self.last_seen
    }

    /// Get room's last seen as ago format
    pub fn ago(&self) -> SharedString {
        self.last_seen.ago()
    }

    /// Sync inbox relays for all room's members
    pub fn verify_inbox_relays(&self, cx: &App) -> Task<Result<Vec<(PublicKey, bool)>, Error>> {
        let client = get_client();
        let pubkeys = self.public_keys();

        cx.background_spawn(async move {
            let mut result = Vec::with_capacity(pubkeys.len());

            for pubkey in pubkeys.into_iter() {
                let filter = Filter::new()
                    .kind(Kind::InboxRelays)
                    .author(pubkey)
                    .limit(1);

                let is_ready = client
                    .database()
                    .query(filter)
                    .await
                    .ok()
                    .and_then(|events| events.first_owned())
                    .is_some();

                result.push((pubkey, is_ready));
            }

            Ok(result)
        })
    }

    /// Send message to all room's members
    ///
    /// NIP-4e: Message will be signed by the device signer
    pub fn send_message(&self, content: String, cx: &App) -> Task<Result<Vec<String>, Error>> {
        let client = get_client();
        let pubkeys = self.public_keys();

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let mut report = vec![];

            let tags: Vec<Tag> = pubkeys
                .iter()
                .filter_map(|pubkey| {
                    if pubkey != &public_key {
                        Some(Tag::public_key(*pubkey))
                    } else {
                        None
                    }
                })
                .collect();

            for pubkey in pubkeys.iter() {
                if let Err(e) = client
                    .send_private_msg(*pubkey, &content, tags.clone())
                    .await
                {
                    report.push(e.to_string());
                }
            }

            Ok(report)
        })
    }

    /// Load metadata for all members
    pub fn load_messages(&self, cx: &App) -> Task<Result<Events, Error>> {
        let client = get_client();
        let pubkeys = self.public_keys();
        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .authors(pubkeys.iter().copied())
            .pubkeys(pubkeys);

        cx.background_spawn(async move {
            let query = client.database().query(filter).await?;
            Ok(query)
        })
    }

    /// Load metadata for all members
    fn load_metadata(event: &Event, cx: &App) -> Task<Result<Vec<NostrProfile>, Error>> {
        let client = get_client();
        let mut pubkeys = vec![];

        // Get all pubkeys from event's tags
        pubkeys.extend(event.tags.public_keys().collect::<HashSet<_>>());
        pubkeys.push(event.pubkey);

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let signer_pubkey = signer.get_public_key().await?;
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

            Ok(profiles)
        })
    }
}
