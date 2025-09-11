use std::cmp::Ordering;
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
    pub output: Option<Output<EventId>>,
    pub local_error: Option<SharedString>,
    pub nip17_relays_not_found: bool,
}

impl SendReport {
    pub fn output(receiver: PublicKey, output: Output<EventId>) -> Self {
        Self {
            receiver,
            output: Some(output),
            local_error: None,
            nip17_relays_not_found: false,
        }
    }

    pub fn error(receiver: PublicKey, error: impl Into<SharedString>) -> Self {
        Self {
            receiver,
            output: None,
            local_error: Some(error.into()),
            nip17_relays_not_found: false,
        }
    }

    pub fn nip17_relays_not_found(receiver: PublicKey) -> Self {
        Self {
            receiver,
            output: None,
            local_error: None,
            nip17_relays_not_found: true,
        }
    }

    pub fn is_relay_error(&self) -> bool {
        self.local_error.is_some() || self.nip17_relays_not_found
    }

    pub fn is_sent_success(&self) -> bool {
        if let Some(output) = self.output.as_ref() {
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

impl Room {
    pub fn new(event: &Event) -> Self {
        let id = event.uniq_id();
        let created_at = event.created_at;

        // Get the members from the event's tags and event's pubkey
        let members = event
            .all_pubkeys()
            .into_iter()
            .unique()
            .sorted()
            .collect_vec();

        // Get the subject from the event's tags
        let subject = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned())
        } else {
            None
        };

        // Get the picture from the event's tags
        let picture = if let Some(tag) = event.tags.find(TagKind::custom("picture")) {
            tag.content().map(|s| s.to_owned())
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
        let (not_match, matches): (Vec<PublicKey>, Vec<PublicKey>) =
            self.members.iter().partition(|&key| key != &rearrange_by);
        self.members = not_match;
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
    pub fn subject(&mut self, subject: String, cx: &mut Context<Self>) {
        self.subject = Some(subject);
        cx.notify();
    }

    /// Updates the picture of the room
    ///
    /// # Arguments
    ///
    /// * `picture` - The new subject to set
    /// * `cx` - The context to notify about the update
    pub fn picture(&mut self, picture: String, cx: &mut Context<Self>) {
        self.picture = Some(picture);
        cx.notify();
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
    /// A string containing the display name
    pub fn display_name(&self, cx: &App) -> String {
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
    /// A string containing the image path or URL
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

        cx.background_spawn(async move {
            let client = nostr_client();

            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .authors(members.clone())
                .pubkeys(members.clone());

            let events: Vec<Event> = client
                .database()
                .query(filter)
                .await?
                .into_iter()
                .filter(|ev| ev.compare_pubkeys(&members))
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
        let mut public_keys = self.members.clone();

        cx.background_spawn(async move {
            let css = css();
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

            // Remove the current public key from the list of receivers
            public_keys.retain(|&pk| pk != public_key);

            // Stored all send errors
            let mut reports = vec![];

            for receiver in public_keys.into_iter() {
                match client
                    .send_private_msg(receiver, &content, tags.clone())
                    .await
                {
                    Ok(output) => {
                        if output
                            .failed
                            .iter()
                            .any(|(_, msg)| msg.starts_with("auth-required:"))
                        {
                            let id = output.id();

                            // Wait for authenticated and resent event successfully
                            for attempt in 0..=SEND_RETRY {
                                // Check if event was successfully resent
                                if let Some(resend_output) = css
                                    .resent_ids
                                    .read()
                                    .await
                                    .iter()
                                    .find(|output| output.id() == id)
                                    .cloned()
                                {
                                    reports.push(SendReport::output(receiver, resend_output));
                                    break;
                                }

                                if attempt == SEND_RETRY {
                                    reports.push(SendReport::output(receiver, output));
                                    break;
                                }

                                smol::Timer::after(Duration::from_secs(1)).await;
                            }
                        } else {
                            reports.push(SendReport::output(receiver, output));
                        }
                    }
                    Err(e) => {
                        if let nostr_sdk::client::Error::PrivateMsgRelaysNotFound = e {
                            reports.push(SendReport::nip17_relays_not_found(receiver));
                        } else {
                            reports.push(SendReport::error(receiver, e.to_string()));
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
                        reports.push(SendReport::output(public_key, output));
                    }
                    Err(e) => {
                        if let nostr_sdk::client::Error::PrivateMsgRelaysNotFound = e {
                            reports.push(SendReport::nip17_relays_not_found(public_key));
                        } else {
                            reports.push(SendReport::error(public_key, e.to_string()));
                        }
                    }
                }
            }

            Ok(reports)
        })
    }
}
