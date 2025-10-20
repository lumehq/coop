use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Error};
use flume::{Receiver, Sender};
use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use smol::lock::RwLock;

use crate::constants::{
    BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT, METADATA_BATCH_TIMEOUT, SEARCH_RELAYS,
};
use crate::paths::config_dir;

const TIMEOUT: u64 = 5;

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
    /// NIP-4e
    ///
    /// A signal to notify UI that the user has not set encryption keys yet
    EncryptionNotSet,

    /// NIP-4e
    ///
    /// A signal to notify UI that the user has set encryption keys
    EncryptionSet(PublicKey),

    /// A signal to notify UI that the client's signer has been set
    SignerSet(PublicKey),

    /// A signal to notify UI that the relay requires authentication
    Auth(AuthRequest),

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

#[derive(Debug, Clone)]
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

    pub fn sender(&self) -> &Sender<SignalKind> {
        &self.tx
    }

    pub async fn send(&self, kind: SignalKind) {
        if let Err(e) = self.tx.send_async(kind).await {
            log::error!("Failed to send signal: {e}");
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug)]
pub struct AppState {
    /// A client to interact with Nostr
    client: Client,

    /// Tracks activity related to Nostr events
    event_tracker: RwLock<EventTracker>,

    /// Signal channel for communication between Nostr and GPUI
    signal: Signal,

    /// Ingester channel for processing public keys
    ingester: Ingester,

    /// The timestamp when the application was initialized.
    pub initialized_at: Timestamp,

    /// Whether gift wrap processing is in progress.
    pub gift_wrap_processing: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let lmdb =
            NostrLMDB::open(config_dir().join("nostr")).expect("Database is NOT initialized");

        let opts = ClientOptions::new()
            .gossip(true)
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(600),
            });

        let client = ClientBuilder::default().database(lmdb).opts(opts).build();
        let event_tracker = RwLock::new(EventTracker::default());

        let signal = Signal::default();
        let ingester = Ingester::default();

        Self {
            client,
            event_tracker,
            signal,
            ingester,
            initialized_at: Timestamp::now(),
            gift_wrap_processing: AtomicBool::new(false),
        }
    }

    /// Returns a reference to the nostr client
    pub fn client(&'static self) -> &'static Client {
        &self.client
    }

    /// Returns a reference to the event tracker
    pub fn tracker(&'static self) -> &'static RwLock<EventTracker> {
        &self.event_tracker
    }

    /// Returns a reference to the signal channel
    pub fn signal(&'static self) -> &'static Signal {
        &self.signal
    }

    /// Returns a reference to the ingester channel
    pub fn ingester(&'static self) -> &'static Ingester {
        &self.ingester
    }

    /// Observes the signer and notifies the app when it's set
    pub async fn observe_signer(&'static self) {
        let client = self.client();
        let loop_duration = Duration::from_millis(800);

        loop {
            if let Ok(signer) = client.signer().await {
                if let Ok(pk) = signer.get_public_key().await {
                    // Notify the app that the signer has been set
                    self.signal().send(SignalKind::SignerSet(pk)).await;

                    // Get user's gossip relays
                    self.get_nip65(pk).await.ok();

                    // Exit the current loop
                    break;
                }
            }

            smol::Timer::after(loop_duration).await;
        }
    }

    /// Observes the gift wrap status and notifies the app when it's set
    pub async fn observe_giftwrap(&'static self) {
        let client = self.client();
        let loop_duration = Duration::from_secs(20);
        let mut is_start_processing = false;
        let mut total_loops = 0;

        loop {
            if client.has_signer().await {
                total_loops += 1;

                if self.gift_wrap_processing.load(Ordering::Acquire) {
                    is_start_processing = true;

                    // Reset gift wrap processing flag
                    let _ = self.gift_wrap_processing.compare_exchange(
                        true,
                        false,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );

                    let signal = SignalKind::GiftWrapStatus(UnwrappingStatus::Processing);
                    self.signal().send(signal).await;
                } else {
                    // Only run further if we are already processing
                    // Wait until after 2 loops to prevent exiting early while events are still being processed
                    if is_start_processing && total_loops >= 2 {
                        let signal = SignalKind::GiftWrapStatus(UnwrappingStatus::Complete);
                        self.signal().send(signal).await;

                        // Reset the counter
                        is_start_processing = false;
                        total_loops = 0;
                    }
                }
            }

            smol::Timer::after(loop_duration).await;
        }
    }

    /// Handles events from the nostr client
    pub async fn handle_notifications(&self) -> Result<(), Error> {
        // Get all bootstrapping relays
        let mut urls = vec![];
        urls.extend(BOOTSTRAP_RELAYS);
        urls.extend(SEARCH_RELAYS);

        // Add relay to the relay pool
        for url in urls.into_iter() {
            self.client.add_relay(url).await?;
        }

        // Establish connection to relays
        self.client.connect().await;

        let mut processed_events: HashSet<EventId> = HashSet::new();
        let mut challenges: HashSet<Cow<'_, str>> = HashSet::new();
        let mut notifications = self.client.notifications();

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
                        // Encryption Keys announcement event
                        Kind::Custom(10044) => {
                            if let Ok(true) = self.is_self_authored(&event).await {
                                if let Some(public_key) = event
                                    .tags
                                    .find(TagKind::custom("n"))
                                    .and_then(|tag| tag.content())
                                    .and_then(|c| PublicKey::parse(c).ok())
                                {
                                    self.signal
                                        .send(SignalKind::EncryptionSet(public_key))
                                        .await;
                                }
                            }
                        }
                        Kind::RelayList => {
                            // Get events if relay list belongs to current user
                            if let Ok(true) = self.is_self_authored(&event).await {
                                let author = event.pubkey;

                                // Fetch user's metadata event
                                if let Err(e) = self.subscribe(author, Kind::Metadata).await {
                                    log::error!("Failed to subscribe to metadata event: {e}");
                                }

                                // Fetch user's contact list event
                                if let Err(e) = self.subscribe(author, Kind::ContactList).await {
                                    log::error!("Failed to subscribe to contact list event: {e}");
                                }

                                // Fetch user's encryption announcement event
                                if let Err(e) = self.get_encryption(author).await {
                                    log::error!("Failed to fetch encryption event: {e}");
                                }

                                // Fetch user's messaging relays event
                                if let Err(e) = self.get_nip17(author).await {
                                    log::error!("Failed to fetch messaging relays event: {e}");
                                }
                            }
                        }
                        Kind::InboxRelays => {
                            // Subscribe to gift wrap events if messaging relays belong to the current user
                            if let Ok(true) = self.is_self_authored(&event).await {
                                let urls: Vec<RelayUrl> =
                                    nip17::extract_relay_list(event.as_ref()).cloned().collect();

                                if let Err(e) = self.get_messages(event.pubkey, &urls).await {
                                    log::error!("Failed to fetch messages: {e}");
                                }
                            }
                        }
                        Kind::ContactList => {
                            if let Ok(true) = self.is_self_authored(&event).await {
                                let public_keys: HashSet<PublicKey> =
                                    event.tags.public_keys().copied().collect();

                                if let Err(e) = self.get_metadata_for_list(public_keys).await {
                                    log::error!("Failed to get metadata for list: {e}");
                                }
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
                    if subscription_id.as_ref() == &SubscriptionId::new("inbox") {
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

    /// Batch metadata requests into a single subscription
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
                        self.get_metadata_for_list(std::mem::take(&mut batch))
                            .await
                            .ok();
                    }
                }
                BatchEvent::Timeout => {
                    self.get_metadata_for_list(std::mem::take(&mut batch))
                        .await
                        .ok();
                }
                BatchEvent::Closed => {
                    self.get_metadata_for_list(std::mem::take(&mut batch))
                        .await
                        .ok();

                    // Exit the current loop
                    break;
                }
            }
        }
    }

    /// Check if event is published by current user
    async fn is_self_authored(&self, event: &Event) -> Result<bool, Error> {
        let signer = self.client.signer().await?;
        let public_key = signer.get_public_key().await?;

        Ok(public_key == event.pubkey)
    }

    /// Subscribe for events that match the given kind for a given author
    pub async fn subscribe(&self, author: PublicKey, kind: Kind) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let filter = Filter::new().author(author).kind(kind).limit(1);

        // Subscribe to filters from the user's write relays
        self.client.subscribe(filter, Some(opts)).await?;

        Ok(())
    }

    /// Get metadata for a list of public keys
    pub async fn get_metadata_for_list<I>(&self, public_keys: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = PublicKey>,
    {
        let authors: Vec<PublicKey> = public_keys.into_iter().collect();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];

        // Return if the list is empty
        if authors.is_empty() {
            return Err(anyhow!("You need at least one public key".to_string(),));
        }

        let filter = Filter::new()
            .limit(authors.len() * kinds.len() + 20)
            .authors(authors)
            .kinds(kinds);

        // Subscribe to filters to the bootstrap relays
        self.client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    /// Get and verify NIP-65 relays for a given public key
    pub async fn get_nip65(&self, public_key: PublicKey) -> Result<(), Error> {
        let timeout = Duration::from_secs(TIMEOUT);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        // Subscribe to events from the bootstrapping relays
        self.client
            .subscribe_to(BOOTSTRAP_RELAYS, filter.clone(), Some(opts))
            .await?;

        let tx = self.signal.sender().clone();
        let database = self.client.database().clone();

        // Verify the received data after a timeout
        smol::spawn(async move {
            smol::Timer::after(timeout).await;

            if database.count(filter).await.unwrap_or(0) < 1 {
                tx.send_async(SignalKind::GossipRelaysNotFound).await.ok();
            }
        })
        .detach();

        Ok(())
    }

    /// Set NIP-65 relays for a current user
    pub async fn set_nip65(
        &self,
        relays: &[(RelayUrl, Option<RelayMetadata>)],
    ) -> Result<(), Error> {
        let signer = self.client.signer().await?;

        let tags: Vec<Tag> = relays
            .iter()
            .cloned()
            .map(|(url, metadata)| Tag::relay_metadata(url, metadata))
            .collect();

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .sign(&signer)
            .await?;

        // Send event to the public relays
        self.client.send_event_to(BOOTSTRAP_RELAYS, &event).await?;

        // Get NIP-17 relays
        self.get_nip17(event.pubkey).await?;

        Ok(())
    }

    /// Get and verify encryption announcement for a given public key
    pub async fn get_encryption(&self, public_key: PublicKey) -> Result<(), Error> {
        let timeout = Duration::from_secs(TIMEOUT);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new()
            .kind(Kind::Custom(10044))
            .author(public_key)
            .limit(1);

        // Subscribe to events from the bootstrapping relays
        self.client.subscribe(filter.clone(), Some(opts)).await?;

        let tx = self.signal.sender().clone();
        let database = self.client.database().clone();

        // Verify the received data after a timeout
        smol::spawn(async move {
            smol::Timer::after(timeout).await;

            if database.count(filter).await.unwrap_or(0) < 1 {
                tx.send_async(SignalKind::EncryptionNotSet).await.ok();
            }
        })
        .detach();

        Ok(())
    }

    /// Get and verify NIP-17 relays for a given public key
    pub async fn get_nip17(&self, public_key: PublicKey) -> Result<(), Error> {
        let timeout = Duration::from_secs(TIMEOUT);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(public_key)
            .limit(1);

        // Subscribe to events from the bootstrapping relays
        self.client.subscribe(filter.clone(), Some(opts)).await?;

        let tx = self.signal.sender().clone();
        let database = self.client.database().clone();

        // Verify the received data after a timeout
        smol::spawn(async move {
            smol::Timer::after(timeout).await;

            if database.count(filter).await.unwrap_or(0) < 1 {
                tx.send_async(SignalKind::MessagingRelaysNotFound)
                    .await
                    .ok();
            }
        })
        .detach();

        Ok(())
    }

    /// Set NIP-17 relays for a current user
    pub async fn set_nip17(&self, relays: &[RelayUrl]) -> Result<(), Error> {
        let signer = self.client.signer().await?;

        let event = EventBuilder::new(Kind::InboxRelays, "")
            .tags(relays.iter().cloned().map(Tag::relay))
            .sign(&signer)
            .await?;

        // Send event to the public relays
        self.client.send_event(&event).await?;

        // Get all gift wrap events after published event
        self.get_messages(event.pubkey, relays).await?;

        Ok(())
    }

    /// Get all gift wrap events in the messaging relays for a given public key
    pub async fn get_messages(
        &self,
        public_key: PublicKey,
        urls: &[RelayUrl],
    ) -> Result<(), Error> {
        let id = SubscriptionId::new("inbox");
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        // Ensure user's have at least one relay
        if urls.is_empty() {
            return Err(anyhow!("Relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter() {
            self.client.add_relay(url).await?;
            self.client.connect_relay(url).await?;
        }

        // Subscribe to filters to user's messaging relays
        self.client
            .subscribe_with_id_to(urls, id, filter, None)
            .await?;

        Ok(())
    }

    /// Stores an unwrapped event in local database with reference to original
    async fn set_rumor(&self, id: EventId, rumor: &Event) -> Result<(), Error> {
        // Save unwrapped event
        self.client.database().save_event(rumor).await?;

        // Create a reference event pointing to the unwrapped event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, "")
            .tags(vec![Tag::identifier(id), Tag::event(rumor.id)])
            .sign(&Keys::generate())
            .await?;

        // Save reference event
        self.client.database().save_event(&event).await?;

        Ok(())
    }

    /// Retrieves a previously unwrapped event from local database
    async fn get_rumor(&self, id: EventId) -> Result<Event, Error> {
        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(id)
            .limit(1);

        if let Some(event) = self.client.database().query(filter).await?.first_owned() {
            let target_id = event.tags.event_ids().collect::<Vec<_>>()[0];

            if let Some(event) = self.client.database().event_by_id(target_id).await? {
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
        let mut rumor: Option<Event> = None;

        if let Ok(event) = self.get_rumor(gift_wrap.id).await {
            rumor = Some(event);
        } else if let Ok(unwrapped) = self.client.unwrap_gift_wrap(gift_wrap).await {
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
}
