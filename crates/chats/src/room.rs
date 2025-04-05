use std::{collections::HashSet, sync::Arc};

use anyhow::Error;
use common::{
    last_seen::LastSeen,
    profile::NostrProfile,
    utils::{compare, room_hash},
};
use global::get_client;
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::SmallVec;

use crate::message::{Message, RoomMessage};

#[derive(Debug, Clone)]
pub struct IncomingEvent {
    pub event: RoomMessage,
}

#[derive(Clone, Copy, Hash, Debug, PartialEq, Eq, Default)]
pub enum RoomKind {
    Ongoing,
    Trusted,
    #[default]
    Unknown,
}

pub struct Room {
    pub id: u64,
    pub last_seen: LastSeen,
    /// Subject of the room
    pub name: Option<SharedString>,
    /// All members of the room
    pub members: Arc<SmallVec<[NostrProfile; 2]>>,
    /// Kind
    pub kind: RoomKind,
    /// All public keys of the room members
    pubkeys: Vec<PublicKey>,
}

impl EventEmitter<IncomingEvent> for Room {}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Room {
    /// Create a new room from an Nostr Event
    pub fn new(event: &Event, kind: RoomKind) -> Self {
        let id = room_hash(event);
        let last_seen = LastSeen(event.created_at);

        // Get the subject from the event's tags
        let name = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            None
        };

        // Get all public keys from the event's tags
        let mut pubkeys = vec![];
        pubkeys.extend(event.tags.public_keys().collect::<HashSet<_>>());
        pubkeys.push(event.pubkey);

        Self {
            id,
            last_seen,
            name,
            kind,
            members: Arc::new(SmallVec::with_capacity(pubkeys.len())),
            pubkeys,
        }
    }

    /// Get room's id
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
        self.pubkeys.clone()
    }

    /// Get room's display name
    pub fn name(&self) -> Option<SharedString> {
        self.name.clone()
    }

    /// Get room's kind
    pub fn kind(&self) -> RoomKind {
        self.kind
    }

    /// Determine if room is a group
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Get room's last seen
    pub fn last_seen(&self) -> LastSeen {
        self.last_seen
    }

    /// Set room's last seen
    pub fn set_last_seen(&mut self, last_seen: LastSeen, cx: &mut Context<Self>) {
        self.last_seen = last_seen;
        cx.notify();
    }

    /// Get room's last seen as ago format
    pub fn ago(&self) -> SharedString {
        self.last_seen.ago()
    }

    /// Verify messaging_relays for all room's members
    pub fn messaging_relays(&self, cx: &App) -> Task<Result<Vec<(PublicKey, bool)>, Error>> {
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
    pub fn load_metadata(&self, cx: &mut Context<Self>) -> Task<Result<Vec<NostrProfile>, Error>> {
        let client = get_client();
        let pubkeys = self.public_keys();

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let signer_pubkey = signer.get_public_key().await?;
            let mut profiles = Vec::with_capacity(pubkeys.len());

            for public_key in pubkeys.into_iter() {
                let metadata = client
                    .database()
                    .metadata(public_key)
                    .await?
                    .unwrap_or_default();

                // Convert metadata to profile
                let profile = NostrProfile::new(public_key, metadata);

                if public_key == signer_pubkey {
                    // Room's owner always push to the end of the vector
                    profiles.push(profile);
                } else {
                    profiles.insert(0, profile);
                }
            }

            Ok(profiles)
        })
    }

    /// Load room messages
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<RoomMessage>, Error>> {
        let client = get_client();
        let pubkeys = self.public_keys();
        let members = Arc::clone(&self.members);

        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .authors(pubkeys.clone())
            .pubkeys(pubkeys.clone());

        cx.background_spawn(async move {
            let mut messages = vec![];
            let parser = NostrParser::new();

            // Get all events from database
            let events = client
                .database()
                .query(filter)
                .await?
                .into_iter()
                .sorted_by_key(|ev| ev.created_at)
                .filter(|ev| {
                    let mut other_pubkeys = ev.tags.public_keys().copied().collect::<Vec<_>>();
                    other_pubkeys.push(ev.pubkey);
                    // Check if the event is from a member of the room
                    compare(&other_pubkeys, &pubkeys)
                })
                .collect::<Vec<_>>();

            for event in events.into_iter() {
                let mut mentions = vec![];
                let content = event.content.clone();
                let tokens = parser.parse(&content);

                let author = members
                    .iter()
                    .find(|profile| profile.public_key == event.pubkey)
                    .cloned()
                    .unwrap_or_else(|| NostrProfile::new(event.pubkey, Metadata::default()));

                let pubkey_tokens = tokens
                    .filter_map(|token| match token {
                        Token::Nostr(nip21) => match nip21 {
                            Nip21::Pubkey(pubkey) => Some(pubkey),
                            Nip21::Profile(profile) => Some(profile.public_key),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();

                for pubkey in pubkey_tokens {
                    if let Some(profile) =
                        members.iter().find(|profile| profile.public_key == pubkey)
                    {
                        mentions.push(profile.clone());
                    } else {
                        let metadata = client
                            .database()
                            .metadata(pubkey)
                            .await?
                            .unwrap_or_default();

                        mentions.push(NostrProfile::new(pubkey, metadata));
                    }
                }

                let message = Message::new(event.id, content, author, mentions, event.created_at);
                let room_message = RoomMessage::user(message);

                messages.push(room_message);
            }

            Ok(messages)
        })
    }

    /// Emit message to GPUI
    pub fn emit_message(&self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let client = get_client();
        let members = Arc::clone(&self.members);

        let task: Task<Result<RoomMessage, Error>> = cx.background_spawn(async move {
            let parser = NostrParser::new();
            let content = event.content.clone();
            let tokens = parser.parse(&content);
            let mut mentions = vec![];

            let author = members
                .iter()
                .find(|profile| profile.public_key == event.pubkey)
                .cloned()
                .unwrap_or_else(|| NostrProfile::new(event.pubkey, Metadata::default()));

            let pubkey_tokens = tokens
                .filter_map(|token| match token {
                    Token::Nostr(nip21) => match nip21 {
                        Nip21::Pubkey(pubkey) => Some(pubkey),
                        Nip21::Profile(profile) => Some(profile.public_key),
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();

            for pubkey in pubkey_tokens {
                if let Some(profile) = members
                    .iter()
                    .find(|profile| profile.public_key == event.pubkey)
                {
                    mentions.push(profile.clone());
                } else {
                    let metadata = client
                        .database()
                        .metadata(pubkey)
                        .await?
                        .unwrap_or_default();

                    mentions.push(NostrProfile::new(pubkey, metadata));
                }
            }

            let message = Message::new(event.id, content, author, mentions, event.created_at);
            let room_message = RoomMessage::user(message);

            Ok(room_message)
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(message) = task.await {
                cx.update(|_, cx| {
                    this.update(cx, |_, cx| {
                        cx.emit(IncomingEvent { event: message });
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }
}
