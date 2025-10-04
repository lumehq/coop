use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::Duration;

use anyhow::{anyhow, Error};
use common::display::RenderedProfile;
use common::event::EventUtils;
use global::constants::SEND_RETRY;
use global::{app_state, nostr_client};
use gpui::{App, AppContext, Context, EventEmitter, SharedString, SharedUri, Task};
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::Registry;

#[derive(Debug, Clone)]
pub struct SendReport {
    pub receiver: PublicKey,
    pub status: Option<Output<EventId>>,
    pub error: Option<SharedString>,
    pub on_hold: Option<Event>,
    pub relays_not_found: bool,
}

impl SendReport {
    pub fn new(receiver: PublicKey) -> Self {
        Self {
            receiver,
            status: None,
            error: None,
            on_hold: None,
            relays_not_found: false,
        }
    }

    pub fn status(mut self, output: Output<EventId>) -> Self {
        self.status = Some(output);
        self.relays_not_found = false;
        self
    }

    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self.relays_not_found = false;
        self
    }

    pub fn on_hold(mut self, event: Event) -> Self {
        self.on_hold = Some(event);
        self
    }

    pub fn not_found(mut self) -> Self {
        self.relays_not_found = true;
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
        let members = val.all_pubkeys();

        // Get subject from tags
        let subject = val
            .tags
            .find(TagKind::Subject)
            .and_then(|tag| tag.content().map(|s| s.to_owned()));

        Room {
            id,
            created_at,
            subject,
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
        let members = val.all_pubkeys();

        // Get subject from tags
        let subject = val
            .tags
            .find(TagKind::Subject)
            .and_then(|tag| tag.content().map(|s| s.to_owned()));

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
    /// Constructs a new room instance for a private message with the given receiver and tags.
    pub async fn new(subject: Option<String>, receivers: Vec<PublicKey>) -> Result<Self, Error> {
        let client = nostr_client();
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        if receivers.is_empty() {
            return Err(anyhow!("You need to add at least one receiver"));
        };

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
            .build(public_key);

        // Generate event ID
        event.ensure_id();

        Ok(Room::from(&event))
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
    pub fn set_subject(&mut self, subject: String, cx: &mut Context<Self>) {
        self.subject = Some(subject);
        cx.notify();
    }

    /// Returns the members of the room
    pub fn members(&self) -> &Vec<PublicKey> {
        &self.members
    }

    /// Checks if the room has more than two members (group)
    pub fn is_group(&self) -> bool {
        self.members.len() > 2
    }

    /// Gets the display name for the room
    pub fn display_name(&self, cx: &App) -> SharedString {
        if let Some(subject) = self.subject.clone() {
            SharedString::from(subject)
        } else {
            self.merged_name(cx)
        }
    }

    /// Gets the display image for the room
    pub fn display_image(&self, proxy: bool, cx: &App) -> SharedUri {
        if !self.is_group() {
            self.display_member(cx).avatar(proxy)
        } else {
            SharedUri::from("brand/group.png")
        }
    }

    /// Get a single member to represent the room
    ///
    /// This member is always different from the current user.
    fn display_member(&self, cx: &App) -> Profile {
        let registry = Registry::read_global(cx);

        if let Some(public_key) = registry.signer_pubkey() {
            for member in self.members() {
                if member != &public_key {
                    return registry.get_person(member, cx);
                }
            }
        }

        registry.get_person(&self.members[0], cx)
    }

    /// Merge the names of the first two members of the room.
    fn merged_name(&self, cx: &App) -> SharedString {
        let registry = Registry::read_global(cx);

        if self.is_group() {
            let profiles: Vec<Profile> = self
                .members
                .iter()
                .map(|public_key| registry.get_person(public_key, cx))
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

    /// Connects to all members's messaging relays
    pub fn connect(&self, cx: &App) -> Task<Result<HashMap<PublicKey, Vec<RelayUrl>>, Error>> {
        let members = self.members.clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let mut relays: HashMap<PublicKey, Vec<RelayUrl>> = HashMap::new();

            for member in members.into_iter() {
                if member == public_key {
                    continue;
                }

                let urls = Self::messaging_relays(member).await.unwrap_or_default();
                relays.entry(member).or_default().extend(urls);
            }

            Ok(relays)
        })
    }

    pub fn disconnect(&self, relays: Vec<RelayUrl>, cx: &App) -> Task<Result<(), Error>> {
        cx.background_spawn(async move {
            let client = nostr_client();

            for relay in relays.into_iter() {
                client.disconnect_relay(relay).await?;
            }

            Ok(())
        })
    }

    /// Loads all messages for this room from the database
    pub fn load_messages(&self, cx: &App) -> Task<Result<Vec<Event>, Error>> {
        let members = self.members.clone();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let sent_ids = app_state()
                .sent_ids
                .read()
                .await
                .iter()
                .copied()
                .collect_vec();

            // Get seen events from database
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifiers(sent_ids);

            let seen_events = client.database().query(filter).await?;

            // Extract seen event IDs
            let seen_ids: Vec<EventId> = seen_events
                .into_iter()
                .filter_map(|event| event.tags.event_ids().next().copied())
                .collect();

            // Get events that sent by current user
            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .author(public_key)
                .pubkeys(members.clone());

            let sent_events = client.database().query(filter).await?;

            // Get events that received by current user
            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .authors(members)
                .pubkey(public_key);

            let recv_events = client.database().query(filter).await?;

            // Merge events
            let events: Vec<Event> = sent_events
                .merge(recv_events)
                .into_iter()
                .filter(|event| !seen_ids.contains(&event.id))
                .collect();

            Ok(events)
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

    /// Create a new message event (unsigned)
    pub fn create_message(&self, content: &str, replies: &[EventId], cx: &App) -> UnsignedEvent {
        let public_key = Registry::read_global(cx).signer_pubkey().unwrap();
        let subject = self.subject.clone();

        let mut tags = vec![];

        // Add receivers
        //
        // NOTE: current user will be removed from the list of receivers
        for member in self.members.iter() {
            tags.push(Tag::public_key(member.to_owned()));
        }

        // Add subject tag if it's present
        if let Some(subject) = subject {
            tags.push(Tag::from_standardized_without_cell(TagStandard::Subject(
                subject,
            )));
        }

        // Add reply/quote tag
        if replies.len() == 1 {
            tags.push(Tag::event(replies[0]))
        } else {
            for id in replies {
                tags.push(Tag::from_standardized_without_cell(TagStandard::Quote {
                    event_id: id.to_owned(),
                    relay_url: None,
                    public_key: None,
                }))
            }
        }

        // Construct a direct message event
        //
        // WARNING: never send this event to relays
        let mut event = EventBuilder::new(Kind::PrivateDirectMessage, content)
            .tags(tags)
            .build(public_key);

        // Generate event ID
        event.ensure_id();

        event
    }

    /// Create a task to send a message to all room members
    pub fn send_message(
        &self,
        rumor: UnsignedEvent,
        backup: bool,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        let mut members = self.members.clone();

        cx.background_spawn(async move {
            let app_state = app_state();
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Remove the current user's public key from the list of receivers
            // Current user will be handled separately
            members.retain(|&pk| pk != public_key);

            let mut reports: Vec<SendReport> = vec![];

            for receiver in members.into_iter() {
                let rumor = rumor.clone();
                let event = EventBuilder::gift_wrap(&signer, &receiver, rumor, vec![]).await?;

                let Ok(relay_urls) = Self::messaging_relays(receiver).await else {
                    reports.push(SendReport::new(receiver).not_found());
                    continue;
                };

                match client.send_event_to(relay_urls, &event).await {
                    Ok(output) => {
                        let id = output.id().to_owned();
                        let auth_required = output.failed.iter().any(|m| m.1.starts_with("auth-"));
                        let report = SendReport::new(receiver).status(output);

                        if auth_required {
                            // Wait for authenticated and resent event successfully
                            for attempt in 0..=SEND_RETRY {
                                let ids = app_state.resent_ids.read().await;

                                // Check if event was successfully resent
                                if let Some(output) = ids.iter().find(|e| e.id() == &id).cloned() {
                                    let output = SendReport::new(receiver).status(output);
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
                        reports.push(SendReport::new(receiver).error(e.to_string()));
                    }
                }
            }

            // Construct a gift wrap to back up to current user's owned messaging relays
            let rumor = rumor.clone();
            let event = EventBuilder::gift_wrap(&signer, &public_key, rumor, vec![]).await?;

            // Only send a backup message to current user if sent successfully to others
            if reports.iter().all(|r| r.is_sent_success()) && backup {
                if let Ok(relay_urls) = Self::messaging_relays(public_key).await {
                    match client.send_event_to(relay_urls, &event).await {
                        Ok(output) => {
                            reports.push(SendReport::new(public_key).status(output));
                        }
                        Err(e) => {
                            reports.push(SendReport::new(public_key).error(e.to_string()));
                        }
                    }
                } else {
                    reports.push(SendReport::new(public_key).not_found());
                }
            } else {
                reports.push(SendReport::new(public_key).on_hold(event));
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
        cx.background_spawn(async move {
            let client = nostr_client();
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
                    if let Ok(relay_urls) = Self::messaging_relays(receiver).await {
                        match client.send_event_to(relay_urls, &event).await {
                            Ok(output) => {
                                resend_reports.push(SendReport::new(receiver).status(output));
                            }
                            Err(e) => {
                                resend_reports.push(SendReport::new(receiver).error(e.to_string()));
                            }
                        }
                    } else {
                        resend_reports.push(SendReport::new(receiver).not_found());
                    }
                }
            }

            Ok(resend_reports)
        })
    }

    /// Gets write relays for public key
    async fn write_relays(public_key: PublicKey) -> Result<Vec<RelayUrl>, Error> {
        let client = nostr_client();
        let app_state = app_state();
        let mut relay_urls: Vec<RelayUrl> = vec![];
        let mut processed: HashSet<EventId> = HashSet::new();

        if let Some(urls) = app_state.gossip.read().await.nip65.get(&public_key) {
            relay_urls.extend(
                urls.iter()
                    .filter_map(|(url, m)| {
                        if m == &Some(RelayMetadata::Write) || m.is_none() {
                            Some(url.to_owned())
                        } else {
                            None
                        }
                    })
                    .take(3),
            );
        } else {
            let filter = Filter::new()
                .kind(Kind::RelayList)
                .author(public_key)
                .limit(1);

            let mut stream = client
                .stream_events(filter, Duration::from_secs(10))
                .await?;

            if let Some(event) = stream.next().await {
                if processed.insert(event.id) {
                    relay_urls.extend(
                        nip65::extract_relay_list(&event)
                            .filter_map(|(url, m)| {
                                if m == &Some(RelayMetadata::Write) || m.is_none() {
                                    Some(url.to_owned())
                                } else {
                                    None
                                }
                            })
                            .take(3),
                    );
                }
            }
        }

        // Ensure relay list is not empty
        if relay_urls.is_empty() {
            return Err(anyhow!("No relays found"));
        }

        // Add and connect to relays
        for url in relay_urls.iter() {
            client.add_read_relay(url).await.ok();
            client.connect_relay(url).await.ok();
        }

        Ok(relay_urls)
    }

    /// Gets messaging relays for public key
    async fn messaging_relays(public_key: PublicKey) -> Result<Vec<RelayUrl>, Error> {
        let client = nostr_client();
        let app_state = app_state();
        let mut processed: HashSet<EventId> = HashSet::new();
        let mut relay_urls = vec![];

        if let Some(urls) = app_state.gossip.read().await.nip17.get(&public_key) {
            relay_urls.extend(urls.to_owned());
        } else if let Ok(write_relays) = Self::write_relays(public_key).await {
            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            let mut stream = client
                .stream_events_from(write_relays, filter, Duration::from_secs(10))
                .await?;

            if let Some(event) = stream.next().await {
                if processed.insert(event.id) {
                    relay_urls.extend(nip17::extract_owned_relay_list(event).take(3));
                }
            }
        }

        // Ensure relay list is not empty
        if relay_urls.is_empty() {
            return Err(anyhow!("No relays found"));
        }

        // Add and connect to relays
        for url in relay_urls.iter() {
            client.add_read_relay(url).await.ok();
            client.connect_relay(url).await.ok();
        }

        Ok(relay_urls)
    }
}
