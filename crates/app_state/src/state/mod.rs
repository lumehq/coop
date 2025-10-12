use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Error};
use flume::{Receiver, Sender};
use nostr_sdk::prelude::*;
use smol::lock::RwLock;

use crate::constants::{
    BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT, METADATA_BATCH_TIMEOUT, SEARCH_RELAYS,
};
use crate::nostr_client;
use crate::paths::support_dir;
use crate::state::gossip::Gossip;

mod gossip;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthRequest {
    pub url: RelayUrl,
    pub challenge: String,
    pub sending: bool,
}

impl AuthRequest {
    pub fn new(challenge: impl Into<String>, url: RelayUrl) -> Self {
        Self {
            challenge: challenge.into(),
            sending: false,
            url,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnwrappingStatus {
    #[default]
    Initialized,
    Processing,
    Complete,
}

/// Signals sent through the global event channel to notify UI
#[derive(Debug)]
pub enum SignalKind {
    /// A signal to notify UI that the client's signer has been set
    SignerSet(PublicKey),

    /// A signal to notify UI that the client's signer has been unset
    SignerUnset,

    /// A signal to notify UI that the relay requires authentication
    Auth(AuthRequest),

    /// A signal to notify UI that the browser proxy service is down
    ProxyDown,

    /// A signal to notify UI that a new profile has been received
    NewProfile(Profile),

    /// A signal to notify UI that a new gift wrap event has been received
    NewMessage((EventId, Event)),

    /// A signal to notify UI that no messaging relays for current user was found
    MessagingRelaysNotFound,

    /// A signal to notify UI that no gossip relays for current user was found
    GossipRelaysNotFound,

    /// A signal to notify UI that gift wrap status has changed
    GiftWrapStatus(UnwrappingStatus),
}

#[derive(Debug)]
pub struct Signal {
    rx: Receiver<SignalKind>,
    tx: Sender<SignalKind>,
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

impl Signal {
    pub fn new() -> Self {
        let (tx, rx) = flume::bounded::<SignalKind>(2048);
        Self { rx, tx }
    }

    pub fn receiver(&self) -> &Receiver<SignalKind> {
        &self.rx
    }

    pub async fn send(&self, kind: SignalKind) {
        if let Err(e) = self.tx.send_async(kind).await {
            log::error!("Failed to send signal: {e}");
        }
    }
}

#[derive(Debug)]
pub struct Ingester {
    rx: Receiver<PublicKey>,
    tx: Sender<PublicKey>,
}

impl Default for Ingester {
    fn default() -> Self {
        Self::new()
    }
}

impl Ingester {
    pub fn new() -> Self {
        let (tx, rx) = flume::bounded::<PublicKey>(1024);
        Self { rx, tx }
    }

    pub fn receiver(&self) -> &Receiver<PublicKey> {
        &self.rx
    }

    pub async fn send(&self, public_key: PublicKey) {
        if let Err(e) = self.tx.send_async(public_key).await {
            log::error!("Failed to send public key: {e}");
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EventTracker {
    /// Tracking events that have been resent by Coop in the current session
    pub resent_ids: Vec<Output<EventId>>,

    /// Temporarily store events that need to be resent later
    pub resend_queue: HashMap<EventId, RelayUrl>,

    /// Tracking events sent by Coop in the current session
    pub sent_ids: HashSet<EventId>,

    /// Tracking events seen on which relays in the current session
    pub seen_on_relays: HashMap<EventId, HashSet<RelayUrl>>,
}

impl EventTracker {
    pub fn resent_ids(&self) -> &Vec<Output<EventId>> {
        &self.resent_ids
    }

    pub fn resend_queue(&self) -> &HashMap<EventId, RelayUrl> {
        &self.resend_queue
    }

    pub fn sent_ids(&self) -> &HashSet<EventId> {
        &self.sent_ids
    }

    pub fn seen_on_relays(&self) -> &HashMap<EventId, HashSet<RelayUrl>> {
        &self.seen_on_relays
    }
}

/// A simple storage to store all states that using across the application.
#[derive(Debug)]
pub struct AppState {
    /// The timestamp when the application was initialized.
    pub initialized_at: Timestamp,

    /// Whether this is the first run of the application.
    pub is_first_run: AtomicBool,

    /// Whether gift wrap processing is in progress.
    pub gift_wrap_processing: AtomicBool,

    /// Subscription ID for listening to gift wrap events from relays.
    pub gift_wrap_sub_id: SubscriptionId,

    /// Auto-close options for relay subscriptions
    pub auto_close_opts: Option<SubscribeAutoCloseOptions>,

    /// NIP-65: https://github.com/nostr-protocol/nips/blob/master/65.md
    pub gossip: RwLock<Gossip>,

    /// Tracks activity related to Nostr events
    pub event_tracker: RwLock<EventTracker>,

    /// Signal channel for communication between Nostr and GPUI
    pub signal: Signal,

    /// Ingester channel for processing public keys
    pub ingester: Ingester,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let first_run = Self::first_run();
        let initialized_at = Timestamp::now();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let signal = Signal::default();
        let ingester = Ingester::default();

        Self {
            initialized_at,
            signal,
            ingester,
            is_first_run: AtomicBool::new(first_run),
            gift_wrap_sub_id: SubscriptionId::new("inbox"),
            gift_wrap_processing: AtomicBool::new(false),
            auto_close_opts: Some(opts),
            gossip: RwLock::new(Gossip::default()),
            event_tracker: RwLock::new(EventTracker::default()),
        }
    }

    pub async fn handle_notifications(&self) -> Result<(), Error> {
        let client = nostr_client();

        // Get all bootstrapping relays
        let mut urls = vec![];
        urls.extend(BOOTSTRAP_RELAYS);
        urls.extend(SEARCH_RELAYS);

        // Add relay to the relay pool
        for url in urls.into_iter() {
            client.add_relay(url).await?;
        }

        // Establish connection to relays
        client.connect().await;

        let mut processed_events: HashSet<EventId> = HashSet::new();
        let mut challenges: HashSet<Cow<'_, str>> = HashSet::new();
        let mut notifications = client.notifications();

        while let Ok(notification) = notifications.recv().await {
            let RelayPoolNotification::Message { message, relay_url } = notification else {
                continue;
            };

            match message {
                RelayMessage::Event { event, .. } => {
                    // Keep track of which relays have seen this event
                    {
                        let mut event_tracker = self.event_tracker.write().await;
                        event_tracker
                            .seen_on_relays
                            .entry(event.id)
                            .or_default()
                            .insert(relay_url);
                    }

                    // Skip events that have already been processed
                    if !processed_events.insert(event.id) {
                        continue;
                    }

                    match event.kind {
                        Kind::RelayList => {
                            // Update NIP-65 relays for event's public key
                            {
                                let mut gossip = self.gossip.write().await;
                                gossip.insert(&event);
                            }

                            let is_self_authored = Self::is_self_authored(&event).await;

                            // Get events if relay list belongs to current user
                            if is_self_authored {
                                let gossip = self.gossip.read().await;

                                // Fetch user's metadata event
                                gossip.subscribe(event.pubkey, Kind::Metadata).await.ok();

                                // Fetch user's contact list event
                                gossip.subscribe(event.pubkey, Kind::ContactList).await.ok();

                                // Fetch user's messaging relays event
                                gossip.get_nip17(event.pubkey).await.ok();
                            }
                        }
                        Kind::InboxRelays => {
                            // Update NIP-17 relays for event's public key
                            {
                                let mut gossip = self.gossip.write().await;
                                gossip.insert(&event);
                            }

                            let is_self_authored = Self::is_self_authored(&event).await;

                            // Subscribe to gift wrap events if messaging relays belong to the current user
                            if is_self_authored {
                                let gossip = self.gossip.read().await;

                                if gossip.monitor_inbox(event.pubkey).await.is_err() {
                                    self.signal.send(SignalKind::MessagingRelaysNotFound).await;
                                }
                            }
                        }
                        Kind::ContactList => {
                            let is_self_authored = Self::is_self_authored(&event).await;

                            if is_self_authored {
                                let public_keys: HashSet<PublicKey> =
                                    event.tags.public_keys().copied().collect();

                                self.gossip
                                    .read()
                                    .await
                                    .bulk_subscribe(public_keys)
                                    .await
                                    .ok();
                            }
                        }
                        Kind::Metadata => {
                            let metadata = Metadata::from_json(&event.content).unwrap_or_default();
                            let profile = Profile::new(event.pubkey, metadata);

                            self.signal.send(SignalKind::NewProfile(profile)).await;
                        }
                        Kind::GiftWrap => {
                            self.extract_rumor(&event).await;
                        }
                        _ => {}
                    }
                }
                RelayMessage::EndOfStoredEvents(subscription_id) => {
                    if *subscription_id == self.gift_wrap_sub_id {
                        self.signal
                            .send(SignalKind::GiftWrapStatus(UnwrappingStatus::Processing))
                            .await;
                    }
                }
                RelayMessage::Auth { challenge } => {
                    if challenges.insert(challenge.clone()) {
                        // Send a signal to the ingester to handle the auth request
                        self.signal
                            .send(SignalKind::Auth(AuthRequest::new(challenge, relay_url)))
                            .await;
                    }
                }
                RelayMessage::Ok {
                    event_id, message, ..
                } => {
                    let msg = MachineReadablePrefix::parse(&message);
                    let mut event_tracker = self.event_tracker.write().await;

                    // Keep track of events sent by Coop
                    event_tracker.sent_ids.insert(event_id);

                    // Keep track of events that need to be resend after auth
                    if let Some(MachineReadablePrefix::AuthRequired) = msg {
                        event_tracker.resend_queue.insert(event_id, relay_url);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn handle_metadata_batching(&self) {
        let timeout = Duration::from_millis(METADATA_BATCH_TIMEOUT);
        let mut processed_pubkeys: HashSet<PublicKey> = HashSet::new();
        let mut batch: HashSet<PublicKey> = HashSet::new();

        /// Internal events for the metadata batching system
        enum BatchEvent {
            PublicKey(PublicKey),
            Timeout,
            Closed,
        }

        loop {
            let futs = smol::future::or(
                async move {
                    if let Ok(public_key) = self.ingester.receiver().recv_async().await {
                        BatchEvent::PublicKey(public_key)
                    } else {
                        BatchEvent::Closed
                    }
                },
                async move {
                    smol::Timer::after(timeout).await;
                    BatchEvent::Timeout
                },
            );

            match futs.await {
                BatchEvent::PublicKey(public_key) => {
                    // Prevent duplicate keys from being processed
                    if processed_pubkeys.insert(public_key) {
                        batch.insert(public_key);
                    }

                    // Process the batch if it's full
                    if batch.len() >= METADATA_BATCH_LIMIT {
                        let gossip = self.gossip.read().await;
                        gossip.bulk_subscribe(std::mem::take(&mut batch)).await.ok();
                    }
                }
                BatchEvent::Timeout => {
                    let gossip = self.gossip.read().await;
                    gossip.bulk_subscribe(std::mem::take(&mut batch)).await.ok();
                }
                BatchEvent::Closed => {
                    let gossip = self.gossip.read().await;
                    gossip.bulk_subscribe(std::mem::take(&mut batch)).await.ok();

                    // Exit the current loop
                    break;
                }
            }
        }
    }

    async fn is_self_authored(event: &Event) -> bool {
        let client = nostr_client();

        let Ok(signer) = client.signer().await else {
            return false;
        };

        let Ok(public_key) = signer.get_public_key().await else {
            return false;
        };

        public_key == event.pubkey
    }

    /// Stores an unwrapped event in local database with reference to original
    async fn set_rumor(&self, id: EventId, rumor: &Event) -> Result<(), Error> {
        let client = nostr_client();

        // Save unwrapped event
        client.database().save_event(rumor).await?;

        // Create a reference event pointing to the unwrapped event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, "")
            .tags(vec![Tag::identifier(id), Tag::event(rumor.id)])
            .sign(&Keys::generate())
            .await?;

        // Save reference event
        client.database().save_event(&event).await?;

        Ok(())
    }

    /// Retrieves a previously unwrapped event from local database
    async fn get_rumor(&self, id: EventId) -> Result<Event, Error> {
        let client = nostr_client();
        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(id)
            .limit(1);

        if let Some(event) = client.database().query(filter).await?.first_owned() {
            let target_id = event.tags.event_ids().collect::<Vec<_>>()[0];

            if let Some(event) = client.database().event_by_id(target_id).await? {
                Ok(event)
            } else {
                Err(anyhow!("Event not found."))
            }
        } else {
            Err(anyhow!("Event is not cached yet."))
        }
    }

    // Unwraps a gift-wrapped event and processes its contents.
    async fn extract_rumor(&self, gift_wrap: &Event) {
        let client = nostr_client();

        let mut rumor: Option<Event> = None;

        if let Ok(event) = self.get_rumor(gift_wrap.id).await {
            rumor = Some(event);
        } else if let Ok(unwrapped) = client.unwrap_gift_wrap(gift_wrap).await {
            // Sign the unwrapped event with a RANDOM KEYS
            if let Ok(event) = unwrapped.rumor.sign_with_keys(&Keys::generate()) {
                // Save this event to the database for future use.
                if let Err(e) = self.set_rumor(gift_wrap.id, &event).await {
                    log::warn!("Failed to cache unwrapped event: {e}")
                }

                rumor = Some(event);
            }
        }

        if let Some(event) = rumor {
            // Send all pubkeys to the metadata batch to sync data
            for public_key in event.tags.public_keys().copied() {
                self.ingester.send(public_key).await;
            }

            match event.created_at >= self.initialized_at {
                // New message: send a signal to notify the UI
                true => {
                    self.signal
                        .send(SignalKind::NewMessage((gift_wrap.id, event)))
                        .await;
                }
                // Old message: Coop is probably processing the user's messages during initial load
                false => {
                    self.gift_wrap_processing.store(true, Ordering::Release);
                }
            }
        }
    }

    fn first_run() -> bool {
        let flag = support_dir().join(".first_run");
        !flag.exists() && std::fs::write(&flag, "").is_ok()
    }
}
