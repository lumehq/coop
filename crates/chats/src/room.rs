use std::sync::Arc;

use account::Account;
use anyhow::Error;
use chrono::{Local, TimeZone};
use common::{compare, profile::SharedProfile, room_hash};
use global::get_client;
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::{
    constants::{DAYS_IN_MONTH, HOURS_IN_DAY, MINUTES_IN_HOUR, NOW, SECONDS_IN_MINUTE},
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
    pub created_at: Timestamp,
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
    /// Creates a new Room instance from a Nostr event
    ///
    /// # Arguments
    ///
    /// * `event` - The Nostr event containing chat information
    ///
    /// # Returns
    ///
    /// A new Room instance with information extracted from the event
    pub fn new(event: &Event) -> Self {
        let id = room_hash(event);
        let created_at = event.created_at;

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
            created_at,
            subject,
            members,
            kind: RoomKind::Unknown,
        }
    }

    /// Sets the kind of the room
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of room to set
    ///
    /// # Returns
    ///
    /// The room with the updated kind
    pub fn kind(mut self, kind: RoomKind) -> Self {
        self.kind = kind;
        self
    }

    /// Calculates a human-readable representation of the time passed since room creation
    ///
    /// # Returns
    ///
    /// A SharedString representing the relative time since room creation:
    /// - "now" for less than a minute
    /// - "Xm" for minutes
    /// - "Xh" for hours
    /// - "Xd" for days
    /// - Month and day (e.g. "Jan 15") for older dates
    pub fn ago(&self) -> SharedString {
        let input_time = match Local.timestamp_opt(self.created_at.as_u64() as i64, 0) {
            chrono::LocalResult::Single(time) => time,
            _ => return "1m".into(),
        };

        let now = Local::now();
        let duration = now.signed_duration_since(input_time);

        match duration {
            d if d.num_seconds() < SECONDS_IN_MINUTE => NOW.into(),
            d if d.num_minutes() < MINUTES_IN_HOUR => format!("{}m", d.num_minutes()),
            d if d.num_hours() < HOURS_IN_DAY => format!("{}h", d.num_hours()),
            d if d.num_days() < DAYS_IN_MONTH => format!("{}d", d.num_days()),
            _ => input_time.format("%b %d").to_string(),
        }
        .into()
    }

    /// Gets the profile for a specific public key
    ///
    /// # Arguments
    ///
    /// * `public_key` - The public key to get the profile for
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// The Profile associated with the given public key
    pub fn profile_by_pubkey(&self, public_key: &PublicKey, cx: &App) -> Profile {
        ChatRegistry::global(cx).read(cx).profile(public_key, cx)
    }

    /// Gets the first member in the room that isn't the current user
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// The Profile of the first member in the room
    pub fn first_member(&self, cx: &App) -> Profile {
        let account = Account::global(cx).read(cx);
        let Some(profile) = account.profile.clone() else {
            return self.profile_by_pubkey(&self.members[0], cx);
        };

        if let Some(public_key) = self
            .members
            .iter()
            .filter(|&pubkey| pubkey != &profile.public_key())
            .collect::<Vec<_>>()
            .first()
        {
            self.profile_by_pubkey(public_key, cx)
        } else {
            profile
        }
    }

    /// Gets all avatars for members in the room
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A vector of SharedString containing all members' avatars
    pub fn avatars(&self, cx: &App) -> Vec<SharedString> {
        let profiles: Vec<Profile> = self
            .members
            .iter()
            .map(|pubkey| ChatRegistry::global(cx).read(cx).profile(pubkey, cx))
            .collect();

        profiles
            .iter()
            .map(|member| member.shared_avatar())
            .collect()
    }

    /// Gets a formatted string of member names
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A SharedString containing formatted member names:
    /// - For a group chat: "name1, name2, +X" where X is the number of additional members
    /// - For a direct message: just the name of the other person
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
                .map(|profile| profile.shared_name())
                .collect::<Vec<_>>()
                .join(", ");

            if profiles.len() > 2 {
                name = format!("{}, +{}", name, profiles.len() - 2);
            }

            name.into()
        } else {
            self.first_member(cx).shared_name()
        }
    }

    /// Gets the display name for the room
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A SharedString representing the display name:
    /// - The subject of the room if it exists
    /// - Otherwise, the formatted names of the members
    pub fn display_name(&self, cx: &App) -> SharedString {
        if let Some(subject) = self.subject.as_ref() {
            subject.clone()
        } else {
            self.names(cx)
        }
    }

    /// Gets the display image for the room
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// An Option<SharedString> containing the avatar:
    /// - For a direct message: the other person's avatar
    /// - For a group chat: None
    pub fn display_image(&self, cx: &App) -> Option<SharedString> {
        if !self.is_group() {
            Some(self.first_member(cx).shared_avatar())
        } else {
            None
        }
    }

    /// Checks if the room is a group chat
    ///
    /// # Returns
    ///
    /// true if the room has more than 2 members, false otherwise
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Updates the creation timestamp of the room
    ///
    /// # Arguments
    ///
    /// * `created_at` - The new Timestamp to set
    /// * `cx` - The context to notify about the update
    pub fn created_at(&mut self, created_at: Timestamp, cx: &mut Context<Self>) {
        self.created_at = created_at;
        cx.notify();
    }

    /// Fetches metadata for all members in the room
    ///
    /// # Arguments
    ///
    /// * `cx` - The context for the background task
    ///
    /// # Returns
    ///
    /// A Task that resolves to Result<Vec<(PublicKey, Option<Metadata>)>, Error>
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

    /// Checks which members have inbox relays set up
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A Task that resolves to Result<Vec<(PublicKey, bool)>, Error> where
    /// the boolean indicates if the member has inbox relays configured
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

    /// Sends a message to all members in the room
    ///
    /// # Arguments
    ///
    /// * `content` - The content of the message to send
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A Task that resolves to Result<Vec<String>, Error> where the
    /// strings contain error messages for any failed sends
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

    /// Loads all messages for this room from the database
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A Task that resolves to Result<Vec<RoomMessage>, Error> containing
    /// all messages for this room
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<RoomMessage>, Error>> {
        let client = get_client();
        let pubkeys = Arc::clone(&self.members);

        let profiles: Vec<Profile> = pubkeys
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
                let id = event.id;
                let created_at = event.created_at;
                let content = event.content.clone();
                let tokens = parser.parse(&content);

                let author = profiles
                    .iter()
                    .find(|profile| profile.public_key() == event.pubkey)
                    .cloned()
                    .unwrap_or_else(|| Profile::new(event.pubkey, Metadata::default()));

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
                            .find(|profile| profile.public_key() == pubkey)
                            .cloned()
                            .unwrap_or_else(|| Profile::new(pubkey, Metadata::default())),
                    );
                }

                let message = Message::new(id, content, author, created_at).with_mentions(mentions);
                let room_message = RoomMessage::user(message);

                messages.push(room_message);
            }

            Ok(messages)
        })
    }

    /// Emits a message event to the GPUI
    ///
    /// # Arguments
    ///
    /// * `event` - The Nostr event to emit
    /// * `window` - The Window to emit the event to
    /// * `cx` - The context for the room
    ///
    /// # Effects
    ///
    /// Processes the event and emits an IncomingEvent to the UI when complete
    pub fn emit_message(&self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let pubkeys = self.members.clone();
        let profiles: Vec<Profile> = pubkeys
            .iter()
            .map(|pubkey| ChatRegistry::global(cx).read(cx).profile(pubkey, cx))
            .collect();

        let task: Task<Result<RoomMessage, Error>> = cx.background_spawn(async move {
            let parser = NostrParser::new();
            let id = event.id;
            let created_at = event.created_at;
            let content = event.content.clone();
            let tokens = parser.parse(&content);
            let mut mentions = vec![];

            let author = profiles
                .iter()
                .find(|profile| profile.public_key() == event.pubkey)
                .cloned()
                .unwrap_or_else(|| Profile::new(event.pubkey, Metadata::default()));

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
                        .find(|profile| profile.public_key() == pubkey)
                        .cloned()
                        .unwrap_or_else(|| Profile::new(pubkey, Metadata::default())),
                );
            }

            let message = Message::new(id, content, author, created_at).with_mentions(mentions);
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
