use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::Duration;

use anyhow::Error;
use common::display::ReadableProfile;
use common::event::EventUtils;
use global::constants::SEND_RETRY;
use global::{css, nostr_client};
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task};
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::Registry;

#[derive(Debug, Clone)]
pub struct SendReport {
    pub receiver: PublicKey,
    pub tags: Option<Vec<Tag>>,
    pub status: Option<Output<EventId>>,
    pub error: Option<SharedString>,
    pub relays_not_found: bool,
}

impl SendReport {
    pub fn new(receiver: PublicKey) -> Self {
        Self {
            receiver,
            status: None,
            error: None,
            tags: None,
            relays_not_found: false,
        }
    }

    pub fn not_found(mut self) -> Self {
        self.relays_not_found = true;
        self
    }

    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self.relays_not_found = false;
        self
    }

    pub fn status(mut self, output: Output<EventId>) -> Self {
        self.status = Some(output);
        self.relays_not_found = false;
        self
    }

    pub fn tags(mut self, tags: &Vec<Tag>) -> Self {
        self.tags = Some(tags.to_owned());
        self
    }

    pub fn is_relay_error(&self) -> bool {
        self.error.is_some() || self.relays_not_found
    }

    pub fn is_sent_success(&self) -> bool {
        if let Some(output) = self.status.as_ref() {
            !output.success.is_empty()
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub enum RoomSignal {
    NewMessage((EventId, Box<Event>)),
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
    pub subject: Option<String>,
    /// Picture of the room
    pub picture: Option<String>,
    /// All members of the room
    pub members: Vec<PublicKey>,
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

impl Hash for Room {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Eq for Room {}

impl EventEmitter<RoomSignal> for Room {}

impl From<&Event> for Room {
    fn from(val: &Event) -> Self {
        let id = val.uniq_id();
        let created_at = val.created_at;

        // Get the members from the event's tags and event's pubkey
        let members = val
            .all_pubkeys()
            .into_iter()
            .unique()
            .sorted()
            .collect_vec();

        // Get the subject from the event's tags
        let subject = if let Some(tag) = val.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned())
        } else {
            None
        };

        // Get the picture from the event's tags
        let picture = if let Some(tag) = val.tags.find(TagKind::custom("picture")) {
            tag.content().map(|s| s.to_owned())
        } else {
            None
        };

        Room {
            id,
            created_at,
            subject,
            picture,
            members,
            kind: RoomKind::default(),
        }
    }
}

impl From<&UnsignedEvent> for Room {
    fn from(val: &UnsignedEvent) -> Self {
        let id = val.uniq_id();
        let created_at = val.created_at;

        // Get the members from the event's tags and event's pubkey
        let members = val
            .all_pubkeys()
            .into_iter()
            .unique()
            .sorted()
            .collect_vec();

        // Get the subject from the event's tags
        let subject = if let Some(tag) = val.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned())
        } else {
            None
        };

        // Get the picture from the event's tags
        let picture = if let Some(tag) = val.tags.find(TagKind::custom("picture")) {
            tag.content().map(|s| s.to_owned())
        } else {
            None
        };

        Room {
            id,
            created_at,
            subject,
            picture,
            members,
            kind: RoomKind::default(),
        }
    }
}

impl Room {
    pub fn new(receiver: PublicKey, tags: Tags, cx: &App) -> Self {
        let identity = Registry::read_global(cx).identity(cx);

        let mut event = EventBuilder::private_msg_rumor(receiver, "")
            .tags(tags)
            .build(identity.public_key());

        // Ensure event ID is generated
        event.ensure_id();

        Room::from(&event).current_user(identity.public_key())
    }

    /// Constructs a new room instance from an nostr event.
    pub fn from(event: impl Into<Room>) -> Self {
        event.into()
    }

    /// Call this function to ensure the current user is always at the bottom of the members list
    pub fn current_user(mut self, public_key: PublicKey) -> Self {
        let (not_match, matches): (Vec<PublicKey>, Vec<PublicKey>) =
            self.members.iter().partition(|&key| key != &public_key);
        self.members = not_match;
        self.members.extend(matches);
        self
    }

    /// Sets the kind of the room and returns the modified room
    pub fn kind(mut self, kind: RoomKind) -> Self {
        self.kind = kind;
        self
    }

    /// Set the room kind to ongoing
    pub fn set_ongoing(&mut self, cx: &mut Context<Self>) {
        if self.kind != RoomKind::Ongoing {
            self.kind = RoomKind::Ongoing;
            cx.notify();
        }
    }

    /// Checks if the room is a group chat
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Updates the creation timestamp of the room
    pub fn created_at(&mut self, created_at: impl Into<Timestamp>, cx: &mut Context<Self>) {
        self.created_at = created_at.into();
        cx.notify();
    }

    /// Updates the subject of the room
    pub fn subject(&mut self, subject: String, cx: &mut Context<Self>) {
        self.subject = Some(subject);
        cx.notify();
    }

    /// Updates the picture of the room
    pub fn picture(&mut self, picture: String, cx: &mut Context<Self>) {
        self.picture = Some(picture);
        cx.notify();
    }

    /// Gets the display name for the room
    pub fn display_name(&self, cx: &App) -> String {
        if let Some(subject) = self.subject.clone() {
            subject
        } else {
            self.merge_name(cx)
        }
    }

    /// Gets the display image for the room
    pub fn display_image(&self, proxy: bool, cx: &App) -> String {
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
    pub(crate) fn merge_name(&self, cx: &App) -> String {
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

            name
        } else {
            self.first_member(cx).display_name()
        }
    }

    /// Connects to all members' messaging relays
    pub fn connect_relays(
        &self,
        cx: &App,
    ) -> Task<Result<HashMap<PublicKey, Vec<RelayUrl>>, Error>> {
        let members = self.members.clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let timeout = Duration::from_secs(3);
            let mut processed = HashSet::new();
            let mut relays: HashMap<PublicKey, Vec<RelayUrl>> = HashMap::new();

            if let Some((_, members)) = members.split_last() {
                for member in members.iter() {
                    relays.insert(member.to_owned(), vec![]);

                    let filter = Filter::new()
                        .kind(Kind::InboxRelays)
                        .author(member.to_owned())
                        .limit(1);

                    if let Ok(mut stream) = client.stream_events(filter, timeout).await {
                        if let Some(event) = stream.next().await {
                            if processed.insert(event.id) {
                                let urls = nip17::extract_owned_relay_list(event).collect_vec();
                                relays.entry(member.to_owned()).or_default().extend(urls);
                            }
                        }
                    }
                }
            };

            Ok(relays)
        })
    }

    /// Loads all messages for this room from the database
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<Event>, Error>> {
        let members = self.members.clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let public_key = members[members.len() - 1];

            let sent = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key)
                .pubkeys(members.clone());

            let recv = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .authors(members)
                .pubkey(public_key);

            let sent_events = client.database().query(sent).await?;
            let recv_events = client.database().query(recv).await?;
            let events: Vec<Event> = sent_events.merge(recv_events).into_iter().collect();

            Ok(events)
        })
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

    /// Create a task to sends a message to all members in the background
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
        let mut public_keys = self.members.clone();

        cx.background_spawn(async move {
            let css = css();
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let mut tags: Vec<Tag> = public_keys
                .iter()
                .filter_map(|&this| {
                    if this != public_key {
                        Some(Tag::public_key(this))
                    } else {
                        None
                    }
                })
                .collect();

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

            // Remove the current public key from the list of receivers
            public_keys.retain(|&pk| pk != public_key);

            // Stored all send errors
            let mut reports = vec![];

            for pubkey in public_keys.into_iter() {
                match client
                    .send_private_msg(pubkey, &content, tags.clone())
                    .await
                {
                    Ok(output) => {
                        let id = output.id().to_owned();
                        let auth_required = output.failed.iter().any(|m| m.1.starts_with("auth-"));
                        let report = SendReport::new(pubkey).status(output).tags(&tags);

                        if auth_required {
                            // Wait for authenticated and resent event successfully
                            for attempt in 0..=SEND_RETRY {
                                // Check if event was successfully resent
                                if let Some(output) = css
                                    .resent_ids
                                    .read()
                                    .await
                                    .iter()
                                    .find(|e| e.id() == &id)
                                    .cloned()
                                {
                                    let output = SendReport::new(pubkey).status(output).tags(&tags);
                                    reports.push(output);
                                    break;
                                }

                                // Check if retry limit exceeded
                                if attempt == SEND_RETRY {
                                    reports.push(report);
                                    break;
                                }

                                smol::Timer::after(Duration::from_millis(1200)).await;
                            }
                        } else {
                            reports.push(report);
                        }
                    }
                    Err(e) => {
                        if let nostr_sdk::client::Error::PrivateMsgRelaysNotFound = e {
                            reports.push(SendReport::new(pubkey).not_found().tags(&tags));
                        } else {
                            reports.push(SendReport::new(pubkey).error(e.to_string()).tags(&tags));
                        }
                    }
                }
            }

            // Only send a backup message to current user if sent successfully to others
            if reports.iter().all(|r| r.is_sent_success()) && backup {
                match client
                    .send_private_msg(public_key, &content, tags.clone())
                    .await
                {
                    Ok(output) => {
                        reports.push(SendReport::new(public_key).status(output).tags(&tags));
                    }
                    Err(e) => {
                        if let nostr_sdk::client::Error::PrivateMsgRelaysNotFound = e {
                            reports.push(SendReport::new(public_key).not_found());
                        } else {
                            reports
                                .push(SendReport::new(public_key).error(e.to_string()).tags(&tags));
                        }
                    }
                }
            }

            Ok(reports)
        })
    }

    /// Create a task to resend a failed message
    pub fn resend(
        &self,
        reports: Vec<SendReport>,
        message: String,
        backup: bool,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        cx.background_spawn(async move {
            let client = nostr_client();
            let mut resend_reports = vec![];
            let mut resend_tag = vec![];

            for report in reports.into_iter() {
                if let Some(output) = report.status {
                    let id = output.id();
                    let urls: Vec<&RelayUrl> = output.failed.keys().collect();

                    if let Some(event) = client.database().event_by_id(id).await? {
                        for url in urls.into_iter() {
                            let relay = client.pool().relay(url).await?;
                            let id = relay.send_event(&event).await?;
                            let resent: Output<EventId> = Output {
                                val: id,
                                success: HashSet::from([url.to_owned()]),
                                failed: HashMap::new(),
                            };

                            resend_reports.push(SendReport::new(report.receiver).status(resent));
                        }

                        if let Some(tags) = report.tags {
                            resend_tag.extend(tags);
                        }
                    }
                }
            }

            // Only send a backup message to current user if sent successfully to others
            if backup && !resend_reports.is_empty() {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let output = client
                    .send_private_msg(public_key, message, resend_tag)
                    .await?;

                resend_reports.push(SendReport::new(public_key).status(output));
            }

            Ok(resend_reports)
        })
    }

    /// Emits a new message signal to the current room
    pub fn emit_message(&self, gift_wrap_id: EventId, event: Event, cx: &mut Context<Self>) {
        cx.emit(RoomSignal::NewMessage((gift_wrap_id, Box::new(event))));
    }

    /// Emits a signal to refresh the current room's messages.
    pub fn emit_refresh(&mut self, cx: &mut Context<Self>) {
        cx.emit(RoomSignal::Refresh);
    }
}
