use std::sync::Arc;

use account::Account;
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

use crate::{
    message::{Message, RoomMessage},
    ChatRegistry,
};

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
    pub subject: Option<SharedString>,
    /// All members of the room
    pub members: Arc<Vec<PublicKey>>,
    /// Kind
    pub kind: RoomKind,
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

        // Get all pubkeys from the event's tags
        let mut pubkeys: Vec<PublicKey> = event.tags.public_keys().cloned().collect();
        pubkeys.push(event.pubkey);

        // Convert pubkeys into members
        let members = Arc::new(pubkeys.into_iter().unique().sorted().collect());

        // Get the subject from the event's tags
        let subject = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            None
        };

        Self {
            id,
            last_seen,
            subject,
            kind,
            members,
        }
    }

    /// Update room's kind
    pub fn kind(&mut self, kind: RoomKind, cx: &mut Context<Self>) {
        self.kind = kind;
        cx.notify();
    }

    /// Update room's last seen
    pub fn last_seen(&mut self, last_seen: LastSeen, cx: &mut Context<Self>) {
        self.last_seen = last_seen;
        cx.notify();
    }

    /// Get member's profile by public key
    pub fn profile_by_pubkey(&self, public_key: &PublicKey, cx: &App) -> NostrProfile {
        ChatRegistry::global(cx).read(cx).profile(public_key, cx)
    }

    /// Get the first member's profile
    ///
    /// Note: first member always != current user
    pub fn first_member(&self, cx: &App) -> NostrProfile {
        let account = Account::global(cx).read(cx);
        let profile = account.profile.clone().unwrap();

        if let Some(public_key) = self
            .members
            .iter()
            .filter(|&pubkey| pubkey != &profile.public_key)
            .collect::<Vec<_>>()
            .first()
        {
            self.profile_by_pubkey(public_key, cx)
        } else {
            profile
        }
    }

    /// Get all members avatar urls
    ///
    /// Used for displaying the room's facepill in the UI.
    pub fn avatars(&self, cx: &App) -> Vec<SharedString> {
        let profiles: Vec<NostrProfile> = self
            .members
            .iter()
            .map(|pubkey| ChatRegistry::global(cx).read(cx).profile(pubkey, cx))
            .collect();

        profiles
            .iter()
            .map(|member| member.avatar.clone())
            .collect()
    }

    /// Get all members names
    ///
    /// Used for displaying the room's name in the UI.
    pub fn names(&self, cx: &App) -> SharedString {
        if self.is_group() {
            let profiles = self
                .members
                .iter()
                .map(|pubkey| ChatRegistry::global(cx).read(cx).profile(pubkey, cx))
                .collect::<Vec<_>>();

            let mut name = profiles
                .iter()
                .take(2)
                .map(|profile| profile.name.to_string())
                .collect::<Vec<_>>()
                .join(", ");

            if profiles.len() > 2 {
                name = format!("{}, +{}", name, profiles.len() - 2);
            }

            name.into()
        } else {
            self.first_member(cx).name
        }
    }

    /// Get the display name of the room.
    pub fn display_name(&self, cx: &App) -> SharedString {
        if let Some(subject) = self.subject.as_ref() {
            subject.clone()
        } else {
            self.names(cx)
        }
    }

    /// Get the display image of the room.
    pub fn display_image(&self, cx: &App) -> Option<SharedString> {
        if !self.is_group() {
            Some(self.first_member(cx).avatar.clone())
        } else {
            None
        }
    }

    /// Determine if room has more than two members
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Get metadata from database for all members
    #[allow(clippy::type_complexity)]
    pub fn metadata(
        &self,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<(PublicKey, Option<Metadata>)>, Error>> {
        let client = get_client();
        let public_keys = self.members.clone();

        cx.background_spawn(async move {
            let mut output = vec![];

            for public_key in public_keys.iter() {
                let metadata = client.database().metadata(*public_key).await?;
                output.push((*public_key, metadata));
            }

            Ok(output)
        })
    }

    /// Verify messaging_relays for all room's members
    pub fn messaging_relays(&self, cx: &App) -> Task<Result<Vec<(PublicKey, bool)>, Error>> {
        let client = get_client();
        let pubkeys = Arc::clone(&self.members);

        cx.background_spawn(async move {
            let mut result = Vec::with_capacity(pubkeys.len());

            for pubkey in pubkeys.iter() {
                let filter = Filter::new()
                    .kind(Kind::InboxRelays)
                    .author(*pubkey)
                    .limit(1);

                let is_ready = client
                    .database()
                    .query(filter)
                    .await
                    .ok()
                    .and_then(|events| events.first_owned())
                    .is_some();

                result.push((*pubkey, is_ready));
            }

            Ok(result)
        })
    }

    /// Send message to all room's members
    pub fn send_message(&self, content: String, cx: &App) -> Task<Result<Vec<String>, Error>> {
        let client = get_client();
        let pubkeys = self.members.clone();

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

    /// Load room messages
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<RoomMessage>, Error>> {
        let client = get_client();
        let pubkeys = Arc::clone(&self.members);

        let profiles: Vec<NostrProfile> = pubkeys
            .iter()
            .map(|pubkey| ChatRegistry::global(cx).read(cx).profile(pubkey, cx))
            .collect();

        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .authors(pubkeys.to_vec())
            .pubkeys(pubkeys.to_vec());

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

                let author = profiles
                    .iter()
                    .find(|profile| profile.public_key == event.pubkey)
                    .cloned()
                    .unwrap_or_else(|| {
                        NostrProfile::new(event.pubkey).metadata(&Metadata::default())
                    });

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
                    mentions.push(
                        profiles
                            .iter()
                            .find(|profile| profile.public_key == pubkey)
                            .cloned()
                            .unwrap_or_else(|| {
                                NostrProfile::new(pubkey).metadata(&Metadata::default())
                            }),
                    );
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
        let pubkeys = self.members.clone();
        let profiles: Vec<NostrProfile> = pubkeys
            .iter()
            .map(|pubkey| ChatRegistry::global(cx).read(cx).profile(pubkey, cx))
            .collect();

        let task: Task<Result<RoomMessage, Error>> = cx.background_spawn(async move {
            let parser = NostrParser::new();
            let content = event.content.clone();
            let tokens = parser.parse(&content);
            let mut mentions = vec![];

            let author = profiles
                .iter()
                .find(|profile| profile.public_key == event.pubkey)
                .cloned()
                .unwrap_or_else(|| NostrProfile::new(event.pubkey).metadata(&Metadata::default()));

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
                mentions.push(
                    profiles
                        .iter()
                        .find(|profile| profile.public_key == pubkey)
                        .cloned()
                        .unwrap_or_else(|| {
                            NostrProfile::new(pubkey).metadata(&Metadata::default())
                        }),
                );
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
