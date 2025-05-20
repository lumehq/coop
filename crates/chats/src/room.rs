use std::{cmp::Ordering, sync::Arc};

use account::Account;
use anyhow::{anyhow, Error};
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
pub struct Incoming(pub Message);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendError {
    pub profile: Profile,
    pub message: String,
}

#[derive(Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RoomKind {
    Ongoing,
    Trusted,
    #[default]
    Unknown,
}

#[derive(Debug)]
pub struct Room {
    pub id: u64,
    pub created_at: Timestamp,
    /// Subject of the room
    pub subject: Option<SharedString>,
    /// Picture of the room
    pub picture: Option<SharedString>,
    /// All members of the room
    pub members: Arc<Vec<PublicKey>>,
    /// Kind
    pub kind: RoomKind,
}

impl Ord for Room {
    fn cmp(&self, other: &Self) -> Ordering {
        self.created_at.cmp(&other.created_at)
    }
}

impl PartialOrd for Room {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Room {}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl EventEmitter<Incoming> for Room {}

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
        // The author is always put at the end of the vector
        pubkeys.push(event.pubkey);

        // Convert pubkeys into members
        let members = Arc::new(pubkeys.into_iter().unique().sorted().collect());

        // Get the subject from the event's tags
        let subject = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            None
        };

        // Get the picture from the event's tags
        let picture = if let Some(tag) = event.tags.find(TagKind::custom("picture")) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            None
        };

        Self {
            id,
            created_at,
            subject,
            picture,
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
    pub fn display_image(&self, cx: &App) -> SharedString {
        if let Some(picture) = self.picture.as_ref() {
            picture.clone()
        } else if !self.is_group() {
            self.first_member(cx).shared_avatar()
        } else {
            "brand/group.png".into()
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

    /// Updates the subject of the room
    ///
    /// # Arguments
    ///
    /// * `subject` - The new subject to set
    /// * `cx` - The context to notify about the update
    pub fn subject(&mut self, subject: String, cx: &mut Context<Self>) {
        self.subject = Some(subject.into());
        cx.notify();
    }

    /// Updates the picture of the room
    ///
    /// # Arguments
    ///
    /// * `picture` - The new subject to set
    /// * `cx` - The context to notify about the update
    pub fn picture(&mut self, picture: String, cx: &mut Context<Self>) {
        self.picture = Some(picture.into());
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
    pub fn load_metadata(
        &self,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<(PublicKey, Option<Metadata>)>, Error>> {
        let client = get_client();
        let public_keys = Arc::clone(&self.members);

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

                let is_ready = client.database().query(filter).await?.first().is_some();

                result.push((*pubkey, is_ready));
            }

            Ok(result)
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
    /// Processes the event and emits an Incoming to the UI when complete
    pub fn emit_message(&self, event: Event, _window: &mut Window, cx: &mut Context<Self>) {
        let author = ChatRegistry::get_global(cx).profile(&event.pubkey, cx);
        let mentions = extract_mentions(&event.content, cx);
        let message =
            Message::new(event.id, event.content, author, event.created_at).with_mentions(mentions);

        cx.emit(Incoming(message));
    }

    /// Creates a temporary message for optimistic updates
    ///
    /// This constructs an unsigned message with the current user as the author,
    /// extracts any mentions from the content, and packages it as a Message struct.
    /// The message will have a generated ID but hasn't been published to relays.
    ///
    /// # Arguments
    ///
    /// * `content` - The message content text
    /// * `cx` - The application context containing user profile information
    ///
    /// # Returns
    ///
    /// Returns `Some(Message)` containing the temporary message if the current user's profile is available,
    /// or `None` if no account is found.
    pub fn create_temp_message(
        &self,
        content: &str,
        reply_to: Option<EventId>,
        cx: &App,
    ) -> Option<Message> {
        let profile = Account::get_global(cx).profile.clone()?;
        let public_key = profile.public_key();
        let builder = EventBuilder::private_msg_rumor(public_key, content);

        // Add event reference if it's present (replying to another event)
        let mut event = if let Some(id) = reply_to {
            builder.tags(vec![Tag::event(id)]).build(public_key)
        } else {
            builder.build(public_key)
        };

        // Create a unsigned event to convert to Coop Message
        event.ensure_id();

        // Extract all mentions from content
        let mentions = extract_mentions(&event.content, cx);

        Some(
            Message::new(event.id.unwrap(), event.content, profile, event.created_at)
                .with_mentions(mentions),
        )
    }

    /// Sends a message to all members in the background task
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
    pub fn send_in_background(
        &self,
        content: &str,
        reply_to: Option<EventId>,
        cx: &App,
    ) -> Task<Result<Vec<SendError>, Error>> {
        let content = content.to_owned();
        let subject = self.subject.clone();
        let picture = self.picture.clone();
        let public_keys = Arc::clone(&self.members);

        cx.background_spawn(async move {
            let client = get_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let mut reports = vec![];

            let mut tags: Vec<Tag> = public_keys
                .iter()
                .filter_map(|pubkey| {
                    if pubkey != &public_key {
                        Some(Tag::public_key(*pubkey))
                    } else {
                        None
                    }
                })
                .collect();

            // Add event reference if it's present (replying to another event)
            if let Some(id) = reply_to {
                tags.push(Tag::event(id));
            }

            // Add subject tag if it's present
            if let Some(subject) = subject {
                tags.push(Tag::from_standardized(TagStandard::Subject(
                    subject.to_string(),
                )));
            }

            // Add picture tag if it's present
            if let Some(picture) = picture {
                tags.push(Tag::custom(TagKind::custom("picture"), vec![picture]));
            }

            let Some((current_user, receivers)) = public_keys.split_last() else {
                return Err(anyhow!("Something is wrong. Cannot get receivers list."));
            };

            for receiver in receivers.iter() {
                if let Err(e) = client
                    .send_private_msg(*receiver, &content, tags.clone())
                    .await
                {
                    let metadata = client
                        .database()
                        .metadata(*receiver)
                        .await?
                        .unwrap_or_default();
                    let profile = Profile::new(*receiver, metadata);
                    let report = SendError {
                        profile,
                        message: e.to_string(),
                    };

                    reports.push(report);
                }
            }

            // Only send a backup message to current user if there are no issues when sending to others
            if reports.is_empty() {
                if let Err(e) = client
                    .send_private_msg(*current_user, &content, tags.clone())
                    .await
                {
                    let metadata = client
                        .database()
                        .metadata(*current_user)
                        .await?
                        .unwrap_or_default();
                    let profile = Profile::new(*current_user, metadata);
                    let report = SendError {
                        profile,
                        message: e.to_string(),
                    };
                    reports.push(report);
                }
            }

            Ok(reports)
        })
    }
}

pub fn extract_mentions(content: &str, cx: &App) -> Vec<Profile> {
    let parser = NostrParser::new();
    let tokens = parser.parse(content);
    let mut mentions = vec![];

    let profiles = ChatRegistry::get_global(cx).profiles.read(cx);

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

    for pubkey in pubkey_tokens.into_iter() {
        if let Some(metadata) = profiles.get(&pubkey).cloned() {
            mentions.push(Profile::new(pubkey, metadata.unwrap_or_default()));
        }
    }

    mentions
}
