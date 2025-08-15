use std::cmp::Ordering;

use anyhow::{anyhow, Error};
use chrono::{Local, TimeZone};
use common::display::DisplayProfile;
use common::event::EventUtils;
use global::nostr_client;
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task, Window};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::SmallVec;

use crate::Registry;

pub(crate) const NOW: &str = "now";
pub(crate) const SECONDS_IN_MINUTE: i64 = 60;
pub(crate) const MINUTES_IN_HOUR: i64 = 60;
pub(crate) const HOURS_IN_DAY: i64 = 24;
pub(crate) const DAYS_IN_MONTH: i64 = 30;

#[derive(Debug, Clone)]
pub struct SendReport {
    pub receiver: PublicKey,
    pub success: Option<Output<EventId>>,
    pub nip17_relays_not_found: bool,
}

impl SendReport {
    pub fn new(receiver: PublicKey, success: Option<Output<EventId>>) -> Self {
        let nip17_relays_not_found = success.is_none();

        Self {
            receiver,
            success,
            nip17_relays_not_found,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RoomSignal {
    NewMessage(Box<Event>),
    Refresh,
}

#[derive(Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RoomKind {
    Ongoing,
    #[default]
    Request,
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

impl EventEmitter<RoomSignal> for Room {}

impl Room {
    pub fn new(event: &Event) -> Self {
        let id = event.uniq_id();
        let created_at = event.created_at;
        let public_keys = event.all_pubkeys();

        // Convert pubkeys into members
        let members = public_keys.into_iter().unique().sorted().collect();

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
            kind: RoomKind::default(),
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

    /// Sets the rearrange_by field of the room and returns the modified room
    ///
    /// This is a builder-style method that allows chaining room modifications.
    ///
    /// # Arguments
    ///
    /// * `rearrange_by` - The PublicKey to set for rearranging the member list
    ///
    /// # Returns
    ///
    /// The modified Room instance with the new member list after rearrangement
    pub fn rearrange_by(mut self, rearrange_by: PublicKey) -> Self {
        let (not_match, matches): (Vec<PublicKey>, Vec<PublicKey>) = self
            .members
            .into_iter()
            .partition(|key| key != &rearrange_by);
        self.members = not_match.into();
        self.members.extend(matches);
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
            self.merge_name(cx)
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
    /// * `proxy` - Whether to use the proxy for the avatar URL
    /// * `cx` - The application context
    ///
    /// # Returns
    ///
    /// A SharedString containing the image path or URL
    pub fn display_image(&self, proxy: bool, cx: &App) -> SharedString {
        if let Some(picture) = self.picture.as_ref() {
            picture.clone()
        } else if !self.is_group() {
            self.first_member(cx).avatar_url(proxy)
        } else {
            "brand/group.png".into()
        }
    }

    /// Get the first member of the room.
    ///
    /// First member is always different from the current user.
    pub(crate) fn first_member(&self, cx: &App) -> Profile {
        let registry = Registry::read_global(cx);
        registry.get_person(&self.members[0], cx)
    }

    /// Merge the names of the first two members of the room.
    pub(crate) fn merge_name(&self, cx: &App) -> SharedString {
        let registry = Registry::read_global(cx);

        if self.is_group() {
            let profiles = self
                .members
                .iter()
                .map(|pk| registry.get_person(pk, cx))
                .collect::<Vec<_>>();

            let mut name = profiles
                .iter()
                .take(2)
                .map(|p| p.display_name())
                .collect::<Vec<_>>()
                .join(", ");

            if profiles.len() > 2 {
                name = format!("{}, +{}", name, profiles.len() - 2);
            }

            name.into()
        } else {
            self.first_member(cx).display_name()
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
            let database = nostr_client().database();
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
    /// A Task that resolves to Result<Vec<Event>, Error> containing all messages for this room
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<Event>, Error>> {
        let members = self.members.clone();
        let members_clone = members.clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let send = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key)
                .pubkeys(members.clone());

            let recv = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .authors(members)
                .pubkey(public_key);

            let send_events = client.database().query(send).await?;
            let recv_events = client.database().query(recv).await?;

            let events = send_events
                .merge(recv_events)
                .into_iter()
                .sorted_by_key(|ev| ev.created_at)
                .filter(|ev| ev.compare_pubkeys(&members_clone))
                .collect::<Vec<_>>();

            Ok(events)
        })
    }

    /// Emits a new message signal to the current room
    pub fn emit_message(&self, event: Event, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(RoomSignal::NewMessage(Box::new(event)));
    }

    /// Emits a signal to refresh the current room's messages.
    pub fn emit_refresh(&mut self, cx: &mut Context<Self>) {
        cx.emit(RoomSignal::Refresh);
    }

    /// Creates a temporary message for optimistic updates
    ///
    /// The event must not been published to relays.
    pub fn create_temp_message(
        &self,
        receiver: PublicKey,
        content: &str,
        replies: &[EventId],
    ) -> UnsignedEvent {
        let builder = EventBuilder::private_msg_rumor(receiver, content);
        let mut tags = vec![];

        // Add event reference if it's present (replying to another event)
        if replies.len() == 1 {
            tags.push(Tag::event(replies[0]))
        } else {
            for id in replies.iter() {
                tags.push(Tag::from_standardized(TagStandard::Quote {
                    event_id: id.to_owned(),
                    relay_url: None,
                    public_key: None,
                }))
            }
        }

        let mut event = builder.tags(tags).build(receiver);
        // Ensure event ID is set
        event.ensure_id();

        event
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
        replies: Vec<EventId>,
        backup: bool,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        let content = content.to_owned();
        let subject = self.subject.clone();
        let picture = self.picture.clone();
        let public_keys = self.members.clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let mut tags = public_keys
                .iter()
                .filter_map(|pubkey| {
                    if pubkey != &public_key {
                        Some(Tag::public_key(*pubkey))
                    } else {
                        None
                    }
                })
                .collect_vec();

            // Add event reference if it's present (replying to another event)
            if replies.len() == 1 {
                tags.push(Tag::event(replies[0]))
            } else {
                for id in replies.iter() {
                    tags.push(Tag::from_standardized(TagStandard::Quote {
                        event_id: id.to_owned(),
                        relay_url: None,
                        public_key: None,
                    }))
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

            // Stored all send errors
            let mut reports = vec![];

            for receiver in receivers.iter() {
                match client
                    .send_private_msg(*receiver, &content, tags.clone())
                    .await
                {
                    Ok(output) => {
                        reports.push(SendReport::new(*receiver, Some(output)));
                    }
                    Err(e) => {
                        log::error!("Send private message to user {receiver} failed: {e}");
                        reports.push(SendReport::new(*receiver, None));
                    }
                }
            }

            // Only send a backup message to current user if there are no issues when sending to others
            if backup && reports.is_empty() {
                match client
                    .send_private_msg(*current_user, &content, tags.clone())
                    .await
                {
                    Ok(output) => {
                        reports.push(SendReport::new(*current_user, Some(output)));
                    }
                    Err(e) => {
                        log::error!("Send private message to user {current_user} failed: {e}");
                        reports.push(SendReport::new(*current_user, None));
                    }
                }
            }

            Ok(reports)
        })
    }
}
