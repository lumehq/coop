use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::Duration;

use account::Account;
use anyhow::{anyhow, Error};
use common::{EventUtils, RenderedProfile};
use encryption::SignerKind;
use gpui::{App, AppContext, Context, EventEmitter, SharedString, Task};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use person::PersonRegistry;
use state::{client, event_store};

use crate::{NewMessage, SendError, SendReport};

const SEND_RETRY: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SendOptions {
    pub backup: bool,
    pub signer_kind: SignerKind,
}

impl SendOptions {
    pub fn new() -> Self {
        Self {
            backup: true,
            signer_kind: SignerKind::default(),
        }
    }

    pub fn backup(&self) -> bool {
        self.backup
    }
}

impl Default for SendOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum RoomEvent {
    NewMessage(NewMessage),
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
    /// Messaing relays
    pub relays: HashMap<PublicKey, Vec<RelayUrl>>,
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
        let members = val.all_pubkeys();

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
            relays: HashMap::default(),
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
        let account = Account::global(cx);
        let public_key = account.read(cx).public_key();

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
        cx.emit(RoomEvent::NewMessage(message));
    }

    /// Emits a signal to refresh the current room's messages.
    pub fn emit_refresh(&mut self, cx: &mut Context<Self>) {
        cx.emit(RoomEvent::Refresh);
    }

    /// Get messaging relays for each member
    pub fn get_relays(&self, cx: &App) -> Task<Result<HashMap<PublicKey, Vec<RelayUrl>>, Error>> {
        let members = self.members();

        cx.background_spawn(async move {
            let client = client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Construct a filter for each member's relay list
            let filters: Vec<Filter> = members
                .into_iter()
                .filter_map(|member| {
                    if member != public_key {
                        Some(
                            Filter::new()
                                .kind(Kind::InboxRelays)
                                .author(member)
                                .limit(1),
                        )
                    } else {
                        None
                    }
                })
                .collect();

            let mut relays: HashMap<PublicKey, Vec<RelayUrl>> = HashMap::new();
            let mut processed_events: HashSet<EventId> = HashSet::new();

            let mut stream = client
                .stream_events(filters, Duration::from_secs(3))
                .await?;

            while let Some((_url, res)) = stream.next().await {
                let event = res?;

                // Skip if the event has already been processed
                if !processed_events.insert(event.id) {
                    continue;
                }

                let urls: Vec<RelayUrl> = nip17::extract_relay_list(&event).cloned().collect();
                // Extend the relay list for the member
                relays.entry(event.pubkey).or_default().extend(urls);
            }

            Ok(relays)
        })
    }

    /// Get all messages belonging to the room
    pub fn get_messages(&self, cx: &App) -> Task<Result<Vec<UnsignedEvent>, Error>> {
        let conversation_id = self.id.to_string();

        cx.background_spawn(async move {
            let client = client();

            // Construct a filter for messages
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
        // Get current user
        let account = Account::global(cx);
        let public_key = account.read(cx).public_key();

        // Get room's subject
        let subject = self.subject.clone();

        let mut tags = vec![];

        // Add receivers
        //
        // NOTE: current user will be removed from the list of receivers
        for member in self.members.iter() {
            // Get relay hint for the member
            let relay_hint = self
                .relays
                .get(member)
                .and_then(|urls| urls.first().cloned());

            // Construct a public key tag with relay hint
            let tag = TagStandard::PublicKey {
                public_key: member.to_owned(),
                relay_url: relay_hint,
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

    /// Send a rumor event (message) to all room members
    pub fn send(
        &self,
        rumor: &UnsignedEvent,
        opts: &SendOptions,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        // Get all members
        let mut members = self.members();

        // Get relay list
        let relay_list = self.relays.clone();

        let rumor = rumor.to_owned();
        let opts = opts.to_owned();

        cx.background_spawn(async move {
            let client = client();

            // Get current user's signer and public key
            let signer = client.signer().await?;
            let current_user = signer.get_public_key().await?;

            // Remove the current user's public key from the list of receivers
            // the current user will be handled separately
            members.retain(|&member| member != current_user);

            // Collect the send reports
            let mut reports: Vec<SendReport> = vec![];

            for member in members.into_iter() {
                let relays = relay_list.get(&member).cloned().unwrap_or(vec![]);

                // Skip sending if relays are not found
                if relays.is_empty() {
                    reports.push(SendReport::new(member).error(SendError::RelayNotFound));
                    continue;
                }

                // Construct the gift wrap event
                let event =
                    EventBuilder::gift_wrap(&signer, &member, rumor.clone(), vec![]).await?;

                // Send the gift wrap event to the messaging relays
                match client.send_event_to(relays, &event).await {
                    Ok(output) => {
                        let id = output.id().to_owned();
                        let auth = output.failed.iter().any(|(_, s)| s.starts_with("auth-"));
                        let report = SendReport::new(member).status(output);

                        if auth {
                            // Wait for authenticated and resent event successfully
                            for attempt in 0..=SEND_RETRY {
                                let event_store = event_store().read().await;
                                let ids = event_store.resent_ids();

                                // Check if event was successfully resent
                                if let Some(output) = ids.iter().find(|e| e.id() == &id).cloned() {
                                    let output = SendReport::new(member).status(output);
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
                        reports
                            .push(SendReport::new(member).error(SendError::Custom(e.to_string())));
                    }
                }
            }

            // Return early if the user disabled backup.
            //
            // Coop will not send a gift wrap event to the current user.
            if !opts.backup() {
                return Ok(reports);
            }

            // Get the relays for the current user
            let relays = relay_list.get(&current_user).cloned().unwrap_or(vec![]);

            // Skip sending if relays are not found
            if relays.is_empty() {
                reports.push(SendReport::new(current_user).error(SendError::RelayNotFound));
                return Ok(reports);
            }

            // Construct the gift-wrapped event
            let event =
                EventBuilder::gift_wrap(&signer, &current_user, rumor.clone(), vec![]).await?;

            // Only send a backup message to current user if sent successfully to others
            if reports.iter().all(|r| r.is_sent_success()) {
                // Send the event to the messaging relays
                match client.send_event(&event).await {
                    Ok(output) => {
                        reports.push(SendReport::new(current_user).status(output));
                    }
                    Err(e) => {
                        reports.push(
                            SendReport::new(current_user).error(SendError::Custom(e.to_string())),
                        );
                    }
                }
            } else {
                reports.push(SendReport::new(current_user).on_hold(event));
            }

            Ok(reports)
        })
    }

    /// Resend failed message based on the provided reports
    pub fn resend(
        &self,
        reports: Vec<SendReport>,
        cx: &App,
    ) -> Task<Result<Vec<SendReport>, Error>> {
        cx.background_spawn(async move {
            let client = client();
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
                            resend_reports.push(
                                SendReport::new(receiver).error(SendError::Custom(e.to_string())),
                            );
                        }
                    }
                }
            }

            Ok(resend_reports)
        })
    }

    #[allow(dead_code)]
    fn select_signer<T>(kind: &SignerKind, user: T, encryption: Option<T>) -> Result<T, Error>
    where
        T: NostrSigner,
    {
        match kind {
            SignerKind::Encryption => {
                Ok(encryption.ok_or_else(|| anyhow!("No encryption key found"))?)
            }
            SignerKind::User => Ok(user),
            SignerKind::Auto => Ok(encryption.unwrap_or(user)),
        }
    }

    #[allow(dead_code)]
    fn select_receiver(
        kind: &SignerKind,
        member: PublicKey,
        encryption: Option<PublicKey>,
    ) -> Result<PublicKey, Error> {
        match kind {
            SignerKind::Encryption => {
                Ok(encryption.ok_or_else(|| anyhow!("Receiver's encryption key not found"))?)
            }
            SignerKind::User => Ok(member),
            SignerKind::Auto => Ok(encryption.unwrap_or(member)),
        }
    }
}
