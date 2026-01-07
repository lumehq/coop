use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::Duration;

use anyhow::Error;
use common::{EventUtils, RenderedProfile};
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use person::PersonRegistry;
use state::{tracker, NostrRegistry};

use crate::NewMessage;

const SEND_RETRY: usize = 10;

#[derive(Debug, Clone)]
pub struct SendReport {
    pub receiver: PublicKey,
    pub status: Option<Output<EventId>>,
    pub error: Option<SharedString>,
    pub on_hold: Option<Event>,
    pub encryption: bool,
    pub relays_not_found: bool,
    pub device_not_found: bool,
}

impl SendReport {
    pub fn new(receiver: PublicKey) -> Self {
        Self {
            receiver,
            status: None,
            error: None,
            on_hold: None,
            encryption: false,
            relays_not_found: false,
            device_not_found: false,
        }
    }

    pub fn status(mut self, output: Output<EventId>) -> Self {
        self.status = Some(output);
        self
    }

    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn on_hold(mut self, event: Event) -> Self {
        self.on_hold = Some(event);
        self
    }

    pub fn encryption(mut self) -> Self {
        self.encryption = true;
        self
    }

    pub fn relays_not_found(mut self) -> Self {
        self.relays_not_found = true;
        self
    }

    pub fn device_not_found(mut self) -> Self {
        self.device_not_found = true;
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

/// Room event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomEvent {
    /// Incoming message.
    Incoming(NewMessage),
    /// Reloads the current room's messages.
    Reload,
}

/// Room kind.
#[derive(Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RoomKind {
    #[default]
    Request,
    Ongoing,
}

#[derive(Debug)]
pub struct Room {
    /// Conversation ID
    pub id: u64,
    /// The timestamp of the last message in the room
    pub created_at: Timestamp,
    /// Subject of the room
    pub subject: Option<SharedString>,
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

impl EventEmitter<RoomEvent> for Room {}

impl From<&UnsignedEvent> for Room {
    fn from(val: &UnsignedEvent) -> Self {
        let id = val.uniq_id();
        let created_at = val.created_at;

        // Get the members from the event's tags and event's pubkey
        let members = val.extract_public_keys();

        // Get subject from tags
        let subject = val
            .tags
            .find(TagKind::Subject)
            .and_then(|tag| tag.content().map(|s| s.to_owned().into()));

        Room {
            id,
            created_at,
            subject,
            members,
            kind: RoomKind::default(),
        }
    }
}

impl Room {
    /// Constructs a new room with the given receiver and tags.
    pub fn new(subject: Option<String>, author: PublicKey, receivers: Vec<PublicKey>) -> Self {
        // Convert receiver's public keys into tags
        let mut tags: Tags = Tags::from_list(
            receivers
                .iter()
                .map(|pubkey| Tag::public_key(pubkey.to_owned()))
                .collect(),
        );

        // Add subject if it is present
        if let Some(subject) = subject {
            tags.push(Tag::from_standardized_without_cell(TagStandard::Subject(
                subject,
            )));
        }

        let mut event = EventBuilder::new(Kind::PrivateDirectMessage, "")
            .tags(tags)
            .build(author);

        // Generate event ID
        event.ensure_id();

        Room::from(&event)
    }

    /// Sets the kind of the room and returns the modified room
    pub fn kind(mut self, kind: RoomKind) -> Self {
        self.kind = kind;
        self
    }

    /// Sets this room is ongoing conversation
    pub fn set_ongoing(&mut self, cx: &mut Context<Self>) {
        if self.kind != RoomKind::Ongoing {
            self.kind = RoomKind::Ongoing;
            cx.notify();
        }
    }

    /// Updates the creation timestamp of the room
    pub fn set_created_at(&mut self, created_at: impl Into<Timestamp>, cx: &mut Context<Self>) {
        self.created_at = created_at.into();
        cx.notify();
    }

    /// Updates the subject of the room
    pub fn set_subject<T>(&mut self, subject: T, cx: &mut Context<Self>)
    where
        T: Into<SharedString>,
    {
        self.subject = Some(subject.into());
        cx.notify();
    }

    /// Returns the members of the room
    pub fn members(&self) -> Vec<PublicKey> {
        self.members.clone()
    }

    /// Returns the members of the room with their messaging relays
    pub fn members_with_relays(&self, cx: &App) -> Task<Vec<(PublicKey, Vec<RelayUrl>)>> {
        let nostr = NostrRegistry::global(cx);
        let mut tasks = vec![];

        for member in self.members.iter() {
            let task = nostr.read(cx).messaging_relays(member, cx);
            tasks.push((*member, task));
        }

        cx.background_spawn(async move {
            let mut results = vec![];

            for (public_key, task) in tasks.into_iter() {
                let urls = task.await;
                results.push((public_key, urls));
            }

            results
        })
    }

    /// Checks if the room has more than two members (group)
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Gets the display name for the room
    pub fn display_name(&self, cx: &App) -> SharedString {
        if let Some(value) = self.subject.clone() {
            value
        } else {
            self.merged_name(cx)
        }
    }

    /// Gets the display image for the room
    pub fn display_image(&self, proxy: bool, cx: &App) -> SharedString {
        if !self.is_group() {
            self.display_member(cx).avatar(proxy)
        } else {
            SharedString::from("brand/group.png")
        }
    }

    /// Get a member to represent the room
    ///
    /// Display member is always different from the current user.
    pub fn display_member(&self, cx: &App) -> Profile {
        let persons = PersonRegistry::global(cx);
        let nostr = NostrRegistry::global(cx);
        let public_key = nostr.read(cx).identity().read(cx).public_key();

        let target_member = self
            .members
            .iter()
            .find(|&member| member != &public_key)
            .or_else(|| self.members.first())
            .expect("Room should have at least one member");

        persons.read(cx).get(target_member, cx)
    }

    /// Merge the names of the first two members of the room.
    fn merged_name(&self, cx: &App) -> SharedString {
        let persons = PersonRegistry::global(cx);

        if self.is_group() {
            let profiles: Vec<Profile> = self
                .members
                .iter()
                .map(|public_key| persons.read(cx).get(public_key, cx))
                .collect();

            let mut name = profiles
                .iter()
                .take(2)
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join(", ");

            if profiles.len() > 2 {
                name = format!("{}, +{}", name, profiles.len() - 2);
            }

            SharedString::from(name)
        } else {
            self.display_member(cx).display_name()
        }
    }

    /// Emits a new message signal to the current room
    pub fn emit_message(&self, message: NewMessage, cx: &mut Context<Self>) {
        cx.emit(RoomEvent::Incoming(message));
    }

    /// Emits a signal to reload the current room's messages.
    pub fn emit_refresh(&mut self, cx: &mut Context<Self>) {
        cx.emit(RoomEvent::Reload);
    }

    /// Get gossip relays for each member
    pub fn connect(&self, cx: &App) -> Task<Result<(), Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let members = self.members();
        let id = SubscriptionId::new(format!("room-{}", self.id));

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Subscription options
            let opts = SubscribeAutoCloseOptions::default()
                .timeout(Some(Duration::from_secs(2)))
                .exit_policy(ReqExitPolicy::ExitOnEOSE);

            for member in members.into_iter() {
                if member == public_key {
                    continue;
                };

                // Construct a filter for gossip relays
                let filter = Filter::new().kind(Kind::RelayList).author(member).limit(1);

                // Subscribe to get member's gossip relays
                client
                    .subscribe_with_id(id.clone(), filter, Some(opts))
                    .await?;
            }

            Ok(())
        })
    }

    /// Get all messages belonging to the room
    pub fn get_messages(&self, cx: &App) -> Task<Result<Vec<UnsignedEvent>, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let conversation_id = self.id.to_string();

        cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .custom_tag(SingleLetterTag::lowercase(Alphabet::C), conversation_id);

            let messages = client
                .database()
                .query(filter)
                .await?
                .into_iter()
                .filter_map(|event| UnsignedEvent::from_json(&event.content).ok())
                .sorted_by_key(|message| message.created_at)
                .collect();

            Ok(messages)
        })
    }

    /// Create a new message event (unsigned)
    pub fn create_message(&self, content: &str, replies: &[EventId], cx: &App) -> UnsignedEvent {
        let nostr = NostrRegistry::global(cx);

        // Get current user
        let public_key = nostr.read(cx).identity().read(cx).public_key();

        // Get room's subject
        let subject = self.subject.clone();

        let mut tags = vec![];

        // Add receivers
        //
        // NOTE: current user will be removed from the list of receivers
        for member in self.members.iter() {
            // Get relay hint if available
            let relay_url = nostr.read(cx).relay_hint(member, cx);

            // Construct a public key tag with relay hint
            let tag = TagStandard::PublicKey {
                public_key: member.to_owned(),
                relay_url,
                alias: None,
                uppercase: false,
            };

            tags.push(Tag::from_standardized_without_cell(tag));
        }

        // Add subject tag if it's present
        if let Some(value) = subject {
            tags.push(Tag::from_standardized_without_cell(TagStandard::Subject(
                value.to_string(),
            )));
        }

        // Add reply/quote tag
        if replies.len() == 1 {
            tags.push(Tag::event(replies[0]))
        } else {
            for id in replies {
                let tag = TagStandard::Quote {
                    event_id: id.to_owned(),
                    relay_url: None,
                    public_key: None,
                };
                tags.push(Tag::from_standardized_without_cell(tag))
            }
        }

        // Construct a direct message event
        //
        // WARNING: never sign and send this event to relays
        let mut event = EventBuilder::new(Kind::PrivateDirectMessage, content)
            .tags(tags)
            .build(public_key);

        // Ensure the event id has been generated
        event.ensure_id();

        event
    }

    /// Create a task to send a message to all room members
    pub fn send_message(
        &self,
        rumor: &UnsignedEvent,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Get current user's public key and relays
        let current_user = nostr.read(cx).identity().read(cx).public_key();
        let current_user_relays = nostr.read(cx).messaging_relays(&current_user, cx);

        let rumor = rumor.to_owned();

        // Get all members and their messaging relays
        let task = self.members_with_relays(cx);

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let current_user_relays = current_user_relays.await;
            let mut members = task.await;

            // Remove the current user's public key from the list of receivers
            // the current user will be handled separately
            members.retain(|(this, _)| this != &current_user);

            // Collect the send reports
            let mut reports: Vec<SendReport> = vec![];

            for (receiver, relays) in members.into_iter() {
                // Check if there are any relays to send the message to
                if relays.is_empty() {
                    reports.push(SendReport::new(receiver).relays_not_found());
                    continue;
                }

                // Ensure relay connection
                for url in relays.iter() {
                    client.add_relay(url).await?;
                    client.connect_relay(url).await?;
                }

                // Construct the gift wrap event
                let event =
                    EventBuilder::gift_wrap(&signer, &receiver, rumor.clone(), vec![]).await?;

                // Send the gift wrap event to the messaging relays
                match client.send_event_to(relays, &event).await {
                    Ok(output) => {
                        let id = output.id().to_owned();
                        let auth = output.failed.iter().any(|(_, s)| s.starts_with("auth-"));
                        let report = SendReport::new(receiver).status(output);
                        let tracker = tracker().read().await;

                        if auth {
                            // Wait for authenticated and resent event successfully
                            for attempt in 0..=SEND_RETRY {
                                // Check if event was successfully resent
                                if tracker.is_sent_by_coop(&id) {
                                    let output = Output::new(id);
                                    let report = SendReport::new(receiver).status(output);
                                    reports.push(report);
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
                        reports.push(SendReport::new(receiver).error(e.to_string()));
                    }
                }
            }

            // Construct the gift-wrapped event
            let event =
                EventBuilder::gift_wrap(&signer, &current_user, rumor.clone(), vec![]).await?;

            // Only send a backup message to current user if sent successfully to others
            if reports.iter().all(|r| r.is_sent_success()) {
                // Check if there are any relays to send the event to
                if current_user_relays.is_empty() {
                    reports.push(SendReport::new(current_user).relays_not_found());
                    return Ok(reports);
                }

                // Ensure relay connection
                for url in current_user_relays.iter() {
                    client.add_relay(url).await?;
                    client.connect_relay(url).await?;
                }

                // Send the event to the messaging relays
                match client.send_event_to(current_user_relays, &event).await {
                    Ok(output) => {
                        reports.push(SendReport::new(current_user).status(output));
                    }
                    Err(e) => {
                        reports.push(SendReport::new(current_user).error(e.to_string()));
                    }
                }
            } else {
                reports.push(SendReport::new(current_user).on_hold(event));
            }

            Ok(reports)
        })
    }

    /// Create a task to resend a failed message
    pub fn resend_message(
        &self,
        reports: Vec<SendReport>,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        cx.background_spawn(async move {
            let mut resend_reports = vec![];

            for report in reports.into_iter() {
                let receiver = report.receiver;

                // Process failed events
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

                            resend_reports.push(SendReport::new(receiver).status(resent));
                        }
                    }
                }

                // Process the on hold event if it exists
                if let Some(event) = report.on_hold {
                    // Send the event to the messaging relays
                    match client.send_event(&event).await {
                        Ok(output) => {
                            resend_reports.push(SendReport::new(receiver).status(output));
                        }
                        Err(e) => {
                            resend_reports.push(SendReport::new(receiver).error(e.to_string()));
                        }
                    }
                }
            }

            Ok(resend_reports)
        })
    }
}
