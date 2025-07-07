use std::cmp::Ordering;

use anyhow::{anyhow, Error};
use chrono::{Local, TimeZone};
use common::profile::RenderProfile;
use global::shared_state;
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task, Window};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::SmallVec;

use crate::constants::{DAYS_IN_MONTH, HOURS_IN_DAY, MINUTES_IN_HOUR, NOW, SECONDS_IN_MINUTE};
use crate::message::Message;
use crate::ChatRegistry;

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
    pub members: SmallVec<[PublicKey; 2]>,
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

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Room {}

impl EventEmitter<Incoming> for Room {}

impl Room {
    pub fn new(event: &Event) -> Self {
        let id = common::room_hash(event);
        let created_at = event.created_at;

        // Get all pubkeys from the event's tags
        let mut pubkeys: Vec<PublicKey> = event.tags.public_keys().cloned().collect();
        // The author is always put at the end of the vector
        pubkeys.push(event.pubkey);

        // Convert pubkeys into members
        let members = pubkeys.into_iter().unique().sorted().collect();

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

    /// Sets the kind of the room and returns the modified room
    ///
    /// This is a builder-style method that allows chaining room modifications.
    ///
    /// # Arguments
    ///
    /// * `kind` - The RoomKind to set for this room
    ///
    /// # Returns
    ///
    /// The modified Room instance with the new kind
    pub fn kind(mut self, kind: RoomKind) -> Self {
        self.kind = kind;
        self
    }

    /// Set the room kind to ongoing
    ///
    /// # Arguments
    ///
    /// * `cx` - The context to notify about the update
    pub fn set_ongoing(&mut self, cx: &mut Context<Self>) {
        if self.kind != RoomKind::Ongoing {
            self.kind = RoomKind::Ongoing;
            cx.notify();
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
    pub fn created_at(&mut self, created_at: impl Into<Timestamp>, cx: &mut Context<Self>) {
        self.created_at = created_at.into();
        cx.notify();
    }

    /// Updates the subject of the room
    ///
    /// # Arguments
    ///
    /// * `subject` - The new subject to set
    /// * `cx` - The context to notify about the update
    pub fn subject(&mut self, subject: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.subject = Some(subject.into());
        cx.notify();
    }

    /// Updates the picture of the room
    ///
    /// # Arguments
    ///
    /// * `picture` - The new subject to set
    /// * `cx` - The context to notify about the update
    pub fn picture(&mut self, picture: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.picture = Some(picture.into());
        cx.notify();
    }

    /// Returns a human-readable string representing how long ago the room was created
    ///
    /// The string will be formatted differently based on the time elapsed:
    /// - Less than a minute: "now"
    /// - Less than an hour: "Xm" (minutes)
    /// - Less than a day: "Xh" (hours)
    /// - Less than a month: "Xd" (days)
    /// - More than a month: "MMM DD" (month abbreviation and day)
    ///
    /// # Returns
    ///
    /// A SharedString containing the formatted time representation
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

    /// Gets the display name for the room
    ///
    /// If the room has a subject set, that will be used as the display name.
    /// Otherwise, it will generate a name based on the room members.
    ///
    /// # Arguments
    ///
    /// * `cx` - The application context
    ///
    /// # Returns
    ///
    /// A SharedString containing the display name
    pub fn display_name(&self, cx: &App) -> SharedString {
        if let Some(subject) = self.subject.clone() {
            subject
        } else {
            self.names(cx)
        }
    }

    /// Gets the display image for the room
    ///
    /// The image is determined by:
    /// - The room's picture if set
    /// - The first member's avatar for 1:1 chats
    /// - A default group image for group chats
    ///
    /// # Arguments
    ///
    /// * `cx` - The application context
    ///
    /// # Returns
    ///
    /// A SharedString containing the image path or URL
    pub fn display_image(&self, cx: &App) -> SharedString {
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;

        if let Some(picture) = self.picture.as_ref() {
            picture.clone()
        } else if !self.is_group() {
            self.first_member(cx).render_avatar(proxy)
        } else {
            "brand/group.png".into()
        }
    }

    pub(crate) fn first_member(&self, cx: &App) -> Profile {
        let registry = ChatRegistry::read_global(cx);

        if let Some(account) = Identity::get_global(cx).profile() {
            self.members
                .iter()
                .filter(|&pubkey| pubkey != &account.public_key())
                .collect::<Vec<_>>()
                .first()
                .map(|public_key| registry.get_person(public_key, cx))
                .unwrap_or(account)
        } else {
            registry.get_person(&self.members[0], cx)
        }
    }

    pub(crate) fn names(&self, cx: &App) -> SharedString {
        let registry = ChatRegistry::read_global(cx);

        if self.is_group() {
            let profiles = self
                .members
                .iter()
                .map(|public_key| registry.get_person(public_key, cx))
                .collect::<Vec<_>>();

            let mut name = profiles
                .iter()
                .take(2)
                .map(|profile| profile.render_name())
                .collect::<Vec<_>>()
                .join(", ");

            if profiles.len() > 2 {
                name = format!("{}, +{}", name, profiles.len() - 2);
            }

            name.into()
        } else {
            self.first_member(cx).render_name()
        }
    }

    /// Loads all profiles for this room members from the database
    ///
    /// # Arguments
    ///
    /// * `cx` - The App context
    ///
    /// # Returns
    ///
    /// A Task that resolves to Result<Vec<Profile>, Error> containing all profiles for this room
    pub fn load_metadata(&self, cx: &mut Context<Self>) -> Task<Result<Vec<Profile>, Error>> {
        let public_keys = self.members.clone();

        cx.background_spawn(async move {
            let database = shared_state().client().database();
            let mut profiles = vec![];

            for public_key in public_keys.into_iter() {
                let metadata = database.metadata(public_key).await?.unwrap_or_default();
                profiles.push(Profile::new(public_key, metadata));
            }

            Ok(profiles)
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
    /// A Task that resolves to Result<Vec<RoomMessage>, Error> containing all messages for this room
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<Message>, Error>> {
        let pubkeys = self.members.clone();

        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .authors(self.members.clone())
            .pubkeys(self.members.clone());

        cx.background_spawn(async move {
            let mut messages = vec![];
            let parser = NostrParser::new();
            let database = shared_state().client().database();

            // Get all events from database
            let events = database
                .query(filter)
                .await?
                .into_iter()
                .sorted_by_key(|ev| ev.created_at)
                .filter(|ev| {
                    let mut other_pubkeys = ev.tags.public_keys().copied().collect::<Vec<_>>();
                    other_pubkeys.push(ev.pubkey);
                    // Check if the event is belong to a member of the current room
                    common::compare(&other_pubkeys, &pubkeys)
                })
                .collect::<Vec<_>>();

            for event in events.into_iter() {
                let content = event.content.clone();
                let tokens = parser.parse(&content);
                let mut replies_to = vec![];

                for tag in event.tags.filter(TagKind::e()) {
                    if let Some(content) = tag.content() {
                        if let Ok(id) = EventId::from_hex(content) {
                            replies_to.push(id);
                        }
                    }
                }

                for tag in event.tags.filter(TagKind::q()) {
                    if let Some(content) = tag.content() {
                        if let Ok(id) = EventId::from_hex(content) {
                            replies_to.push(id);
                        }
                    }
                }

                let mentions = tokens
                    .filter_map(|token| match token {
                        Token::Nostr(nip21) => match nip21 {
                            Nip21::Pubkey(pubkey) => Some(pubkey),
                            Nip21::Profile(profile) => Some(profile.public_key),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();

                if let Ok(message) = Message::builder(event.id, event.pubkey)
                    .content(content)
                    .created_at(event.created_at)
                    .replies_to(replies_to)
                    .mentions(mentions)
                    .build()
                {
                    messages.push(message);
                }
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
        // Extract all mentions from content
        let mentions = extract_mentions(&event.content);

        // Extract reply_to if present
        let mut replies_to = vec![];

        for tag in event.tags.filter(TagKind::e()) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    replies_to.push(id);
                }
            }
        }

        for tag in event.tags.filter(TagKind::q()) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    replies_to.push(id);
                }
            }
        }

        if let Ok(message) = Message::builder(event.id, event.pubkey)
            .content(event.content)
            .created_at(event.created_at)
            .replies_to(replies_to)
            .mentions(mentions)
            .build()
        {
            cx.emit(Incoming(message));
        }
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
        replies: Option<&Vec<Message>>,
        cx: &App,
    ) -> Option<Message> {
        let author = Identity::get_global(cx).profile()?;
        let public_key = author.public_key();
        let builder = EventBuilder::private_msg_rumor(public_key, content);

        // Add event reference if it's present (replying to another event)
        let mut refs = vec![];

        if let Some(replies) = replies {
            if replies.len() == 1 {
                refs.push(Tag::event(replies[0].id))
            } else {
                for message in replies.iter() {
                    refs.push(Tag::custom(TagKind::q(), vec![message.id]))
                }
            }
        }

        let mut event = if !refs.is_empty() {
            builder.tags(refs).build(public_key)
        } else {
            builder.build(public_key)
        };

        // Create a unsigned event to convert to Coop Message
        event.ensure_id();

        // Extract all mentions from content
        let mentions = extract_mentions(&event.content);

        // Extract reply_to if present
        let mut replies_to = vec![];

        for tag in event.tags.filter(TagKind::e()) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    replies_to.push(id);
                }
            }
        }

        for tag in event.tags.filter(TagKind::q()) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    replies_to.push(id);
                }
            }
        }

        Message::builder(event.id.unwrap(), author.public_key())
            .content(event.content)
            .created_at(event.created_at)
            .replies_to(replies_to)
            .mentions(mentions)
            .build()
            .ok()
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
        replies: Option<&Vec<Message>>,
        cx: &App,
    ) -> Task<Result<Vec<SendError>, Error>> {
        let content = content.to_owned();
        let replies = replies.cloned();
        let subject = self.subject.clone();
        let picture = self.picture.clone();
        let public_keys = self.members.clone();
        let backup = AppSettings::get_global(cx).settings.backup_messages;

        cx.background_spawn(async move {
            let client = shared_state().client();
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
            if let Some(replies) = replies {
                if replies.len() == 1 {
                    tags.push(Tag::event(replies[0].id))
                } else {
                    for message in replies.iter() {
                        tags.push(Tag::custom(TagKind::q(), vec![message.id]))
                    }
                }
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
            if backup && reports.is_empty() {
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

pub(crate) fn extract_mentions(content: &str) -> Vec<PublicKey> {
    let parser = NostrParser::new();
    let tokens = parser.parse(content);

    tokens
        .filter_map(|token| match token {
            Token::Nostr(nip21) => match nip21 {
                Nip21::Pubkey(pubkey) => Some(pubkey),
                Nip21::Profile(profile) => Some(profile.public_key),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
}
