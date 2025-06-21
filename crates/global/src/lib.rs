use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use std::time::Duration;
use std::{fs, mem};

use anyhow::{anyhow, Error};
use constants::{
    ALL_MESSAGES_SUB_ID, APP_ID, APP_PUBKEY, BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT,
    METADATA_BATCH_TIMEOUT, NEW_MESSAGE_SUB_ID, SEARCH_RELAYS,
};
use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::lock::RwLock;
use smol::Task;

use crate::constants::{BATCH_CHANNEL_LIMIT, GLOBAL_CHANNEL_LIMIT};
use crate::paths::support_dir;

pub mod constants;
pub mod paths;

/// Global singleton instance for application state
static GLOBALS: OnceLock<Globals> = OnceLock::new();

/// Signals sent through the global event channel to notify UI components
#[derive(Debug)]
pub enum NostrSignal {
    /// New gift wrap event received
    Event(Event),
    /// Finished processing all gift wrap events
    Finish,
    /// Partially finished processing all gift wrap events
    PartialFinish,
    /// Receives EOSE response from relay pool
    Eose(SubscriptionId),
    /// Notice from Relay Pool
    Notice(String),
    /// Application update event received
    AppUpdate(Event),
}

/// Global application state containing Nostr client and shared resources
pub struct Globals {
    /// The Nostr SDK client
    client: Client,
    /// Determines if this is the first time user run Coop
    first_run: bool,
    /// Cache of user profiles mapped by their public keys
    persons: RwLock<BTreeMap<PublicKey, Option<Metadata>>>,
    /// Channel sender for broadcasting global Nostr events to UI
    global_sender: smol::channel::Sender<NostrSignal>,
    /// Channel receiver for handling global Nostr events
    global_receiver: smol::channel::Receiver<NostrSignal>,

    batch_sender: smol::channel::Sender<PublicKey>,
    batch_receiver: smol::channel::Receiver<PublicKey>,

    event_sender: smol::channel::Sender<Event>,
    event_receiver: smol::channel::Receiver<Event>,
}

/// Returns the global singleton instance, initializing it if necessary
pub fn shared_state() -> &'static Globals {
    GLOBALS.get_or_init(|| {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let first_run = is_first_run().unwrap_or(true);
        let opts = Options::new().gossip(true);
        let lmdb = NostrLMDB::open(nostr_file()).expect("Database is NOT initialized");

        let (global_sender, global_receiver) =
            smol::channel::bounded::<NostrSignal>(GLOBAL_CHANNEL_LIMIT);

        let (batch_sender, batch_receiver) =
            smol::channel::bounded::<PublicKey>(BATCH_CHANNEL_LIMIT);

        let (event_sender, event_receiver) = smol::channel::unbounded::<Event>();

        Globals {
            client: ClientBuilder::default().database(lmdb).opts(opts).build(),
            persons: RwLock::new(BTreeMap::new()),
            first_run,
            global_sender,
            global_receiver,
            batch_sender,
            batch_receiver,
            event_sender,
            event_receiver,
        }
    })
}

impl Globals {
    /// Starts the global event processing system and metadata batching
    pub async fn start(&self) {
        self.connect().await;
        self.preload_metadata().await;
        self.subscribe_for_app_updates().await;
        self.batching_metadata().detach(); // .detach() to keep running in background

        let mut notifications = self.client.notifications();
        let mut processed_events: BTreeSet<EventId> = BTreeSet::new();
        let new_messages_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Message { message, .. } = notification {
                match message {
                    RelayMessage::Event {
                        event,
                        subscription_id,
                    } => {
                        if processed_events.contains(&event.id) {
                            continue;
                        }
                        // Skip events that have already been processed
                        processed_events.insert(event.id);

                        match event.kind {
                            Kind::GiftWrap => {
                                if *subscription_id == new_messages_sub_id {
                                    self.unwrap_event(&event, true).await;
                                } else {
                                    self.event_sender.send(event.into_owned()).await.ok();
                                }
                            }
                            Kind::Metadata => {
                                self.insert_person_from_event(&event).await;
                            }
                            Kind::ContactList => {
                                self.extract_pubkeys_and_sync(&event).await;
                            }
                            Kind::ReleaseArtifactSet => {
                                self.notify_update(&event).await;
                            }
                            _ => {}
                        }
                    }
                    RelayMessage::EndOfStoredEvents(subscription_id) => {
                        self.global_sender
                            .send(NostrSignal::Eose(subscription_id.into_owned()))
                            .await
                            .ok();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Gets a reference to the Nostr Client instance
    pub fn client(&'static self) -> &'static Client {
        &self.client
    }

    /// Gets the global signal receiver
    pub fn signal(&self) -> smol::channel::Receiver<NostrSignal> {
        self.global_receiver.clone()
    }

    /// Returns whether this is the first time the application has been run
    pub fn first_run(&self) -> bool {
        self.first_run
    }

    /// Batch metadata requests. Combine all requests from multiple authors into single filter
    pub(crate) fn batching_metadata(&self) -> Task<()> {
        smol::spawn(async move {
            let duration = Duration::from_millis(METADATA_BATCH_TIMEOUT);
            let mut batch: BTreeSet<PublicKey> = BTreeSet::new();

            loop {
                let timeout = smol::Timer::after(duration);
                /// Internal events for the metadata batching system
                enum BatchEvent {
                    NewKeys(PublicKey),
                    Timeout,
                    Closed,
                }

                let event = smol::future::or(
                    async {
                        if let Ok(public_key) = shared_state().batch_receiver.recv().await {
                            BatchEvent::NewKeys(public_key)
                        } else {
                            BatchEvent::Closed
                        }
                    },
                    async {
                        timeout.await;
                        BatchEvent::Timeout
                    },
                )
                .await;

                match event {
                    BatchEvent::NewKeys(public_key) => {
                        batch.insert(public_key);
                        // Process immediately if batch limit reached
                        if batch.len() >= METADATA_BATCH_LIMIT {
                            shared_state()
                                .sync_data_for_pubkeys(mem::take(&mut batch))
                                .await;
                        }
                    }
                    BatchEvent::Timeout => {
                        if !batch.is_empty() {
                            shared_state()
                                .sync_data_for_pubkeys(mem::take(&mut batch))
                                .await;
                        }
                    }
                    BatchEvent::Closed => {
                        if !batch.is_empty() {
                            shared_state()
                                .sync_data_for_pubkeys(mem::take(&mut batch))
                                .await;
                        }
                        break;
                    }
                }
            }
        })
    }

    /// Process to unwrap the gift wrapped events
    pub(crate) fn process_gift_wrap_events(&self) -> Task<()> {
        smol::spawn(async move {
            let timeout_duration = Duration::from_millis(700);
            let mut counter = 0;

            loop {
                if shared_state().client.signer().await.is_err() {
                    break;
                }

                let timeout = smol::Timer::after(timeout_duration);
                let event = smol::future::or(
                    async { (shared_state().event_receiver.recv().await).ok() },
                    async {
                        timeout.await;
                        None
                    },
                )
                .await;

                match event {
                    Some(event) => {
                        // Process the gift wrap event unwrapping
                        shared_state().unwrap_event(&event, false).await;
                        // Increment the total messages counter
                        counter += 1;
                        // Send partial finish signal to GPUI
                        if counter >= 20 {
                            shared_state()
                                .global_sender
                                .send(NostrSignal::PartialFinish)
                                .await
                                .ok();
                        }
                    }
                    None => {
                        shared_state()
                            .global_sender
                            .send(NostrSignal::Finish)
                            .await
                            .ok();

                        break;
                    }
                }
            }

            // Event channel is no longer needed when all gift wrap events have been processed
            shared_state().event_receiver.close();
        })
    }

    /// Gets a person's profile from cache or creates default (blocking)
    pub fn person(&self, public_key: &PublicKey) -> Profile {
        let metadata = if let Some(metadata) = self.persons.read_blocking().get(public_key) {
            metadata.clone().unwrap_or_default()
        } else {
            Metadata::default()
        };

        Profile::new(*public_key, metadata)
    }

    /// Gets a person's profile from cache or creates default (async)
    pub async fn async_person(&self, public_key: &PublicKey) -> Profile {
        let metadata = if let Some(metadata) = self.persons.read().await.get(public_key) {
            metadata.clone().unwrap_or_default()
        } else {
            Metadata::default()
        };

        Profile::new(*public_key, metadata)
    }

    /// Check if a person exists or not
    pub async fn has_person(&self, public_key: &PublicKey) -> bool {
        self.persons.read().await.contains_key(public_key)
    }

    /// Inserts or updates a person's metadata
    pub async fn insert_person(&self, public_key: PublicKey, metadata: Option<Metadata>) {
        self.persons
            .write()
            .await
            .entry(public_key)
            .and_modify(|entry| {
                if entry.is_none() {
                    *entry = metadata.clone();
                }
            })
            .or_insert_with(|| metadata);
    }

    /// Inserts or updates a person's metadata from a Kind::Metadata event
    pub(crate) async fn insert_person_from_event(&self, event: &Event) {
        let metadata = Metadata::from_json(&event.content).ok();

        self.persons
            .write()
            .await
            .entry(event.pubkey)
            .and_modify(|entry| {
                if entry.is_none() {
                    *entry = metadata.clone();
                }
            })
            .or_insert_with(|| metadata);
    }

    /// Connects to bootstrap and configured relays
    pub(crate) async fn connect(&self) {
        for relay in BOOTSTRAP_RELAYS.into_iter() {
            if let Err(e) = self.client.add_relay(relay).await {
                log::error!("Failed to add relay {}: {}", relay, e);
            }
        }

        for relay in SEARCH_RELAYS.into_iter() {
            if let Err(e) = self.client.add_relay(relay).await {
                log::error!("Failed to add relay {}: {}", relay, e);
            }
        }

        // Establish connection to relays
        self.client.connect().await;

        log::info!("Connected to bootstrap relays");
    }

    /// Subscribes to user-specific data feeds (DMs, mentions, etc.)
    pub async fn subscribe_for_user_data(&self, public_key: PublicKey) {
        let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
        let new_messages_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        self.client
            .subscribe(
                Filter::new()
                    .author(public_key)
                    .kinds(vec![
                        Kind::Metadata,
                        Kind::ContactList,
                        Kind::MuteList,
                        Kind::SimpleGroups,
                        Kind::InboxRelays,
                        Kind::RelayList,
                    ])
                    .since(Timestamp::now()),
                None,
            )
            .await
            .ok();

        self.client
            .subscribe(
                Filter::new()
                    .kinds(vec![
                        Kind::Metadata,
                        Kind::ContactList,
                        Kind::InboxRelays,
                        Kind::MuteList,
                        Kind::SimpleGroups,
                    ])
                    .author(public_key)
                    .limit(10),
                Some(SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE)),
            )
            .await
            .ok();

        self.client
            .subscribe_with_id(
                all_messages_sub_id,
                Filter::new().kind(Kind::GiftWrap).pubkey(public_key),
                Some(opts),
            )
            .await
            .ok();

        self.client
            .subscribe_with_id(
                new_messages_sub_id,
                Filter::new()
                    .kind(Kind::GiftWrap)
                    .pubkey(public_key)
                    .limit(0),
                None,
            )
            .await
            .ok();

        log::info!("Getting all user's metadata and messages...");
        // Process gift-wrapped events in the background
        self.process_gift_wrap_events().detach();
    }

    /// Subscribes to application update notifications
    pub(crate) async fn subscribe_for_app_updates(&self) {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let coordinate = Coordinate {
            kind: Kind::Custom(32267),
            public_key: PublicKey::from_hex(APP_PUBKEY).expect("App Pubkey is invalid"),
            identifier: APP_ID.into(),
        };
        let filter = Filter::new()
            .kind(Kind::ReleaseArtifactSet)
            .coordinate(&coordinate)
            .limit(1);

        if let Err(e) = self
            .client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await
        {
            log::error!("Failed to subscribe for app updates: {}", e);
        }

        log::info!("Subscribed to app updates");
    }

    pub(crate) async fn preload_metadata(&self) {
        let filter = Filter::new().kind(Kind::Metadata).limit(100);
        if let Ok(events) = self.client.database().query(filter).await {
            for event in events.into_iter() {
                self.insert_person_from_event(&event).await;
            }
        }
    }

    /// Stores an unwrapped event in local database with reference to original
    pub(crate) async fn set_unwrapped(
        &self,
        root: EventId,
        event: &Event,
        keys: &Keys,
    ) -> Result<(), Error> {
        // Must be use the random generated keys to sign this event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, event.as_json())
            .tags(vec![Tag::identifier(root), Tag::event(root)])
            .sign(keys)
            .await?;

        // Only save this event into the local database
        self.client.database().save_event(&event).await?;

        Ok(())
    }

    /// Retrieves a previously unwrapped event from local database
    pub(crate) async fn get_unwrapped(&self, target: EventId) -> Result<Event, Error> {
        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(target)
            .event(target)
            .limit(1);

        if let Some(event) = self.client.database().query(filter).await?.first_owned() {
            Ok(Event::from_json(event.content)?)
        } else {
            Err(anyhow!("Event is not cached yet"))
        }
    }

    /// Unwraps a gift-wrapped event and processes its contents
    pub(crate) async fn unwrap_event(&self, event: &Event, incoming: bool) {
        let event = match self.get_unwrapped(event.id).await {
            Ok(event) => event,
            Err(_) => {
                let keys = Keys::generate();
                match self.client.unwrap_gift_wrap(event).await {
                    Ok(unwrap) => {
                        let Ok(unwrapped) = unwrap.rumor.sign_with_keys(&keys) else {
                            return;
                        };
                        // Save this event to the database for future use.
                        _ = self.set_unwrapped(event.id, &unwrapped, &keys).await;

                        unwrapped
                    }
                    Err(_) => return,
                }
            }
        };

        // Save the event to the database, use for query directly.
        if let Err(e) = self.client.database().save_event(&event).await {
            log::error!("Failed to save event: {e}")
        }

        // Send all pubkeys to the batch to sync metadata
        self.batch_sender.send(event.pubkey).await.ok();

        for public_key in event.tags.public_keys().copied() {
            self.batch_sender.send(public_key).await.ok();
        }

        // Send a notify to GPUI if this is a new message
        if incoming {
            self.global_sender
                .send(NostrSignal::Event(event))
                .await
                .ok();
        }
    }

    /// Extracts public keys from contact list and queues metadata sync
    pub(crate) async fn extract_pubkeys_and_sync(&self, event: &Event) {
        if let Ok(signer) = self.client.signer().await {
            if let Ok(public_key) = signer.get_public_key().await {
                if public_key == event.pubkey {
                    for public_key in event.tags.public_keys().copied() {
                        self.batch_sender.send(public_key).await.ok();
                    }
                }
            }
        }
    }

    /// Fetches metadata for a batch of public keys
    pub(crate) async fn sync_data_for_pubkeys(&self, public_keys: BTreeSet<PublicKey>) {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![
            Kind::Metadata,
            Kind::ContactList,
            Kind::InboxRelays,
            Kind::UserStatus,
        ];
        let filter = Filter::new()
            .limit(public_keys.len() * kinds.len())
            .authors(public_keys)
            .kinds(kinds);

        if let Err(e) = shared_state()
            .client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await
        {
            log::error!("Failed to sync metadata: {e}");
        }
    }

    /// Notifies UI of application updates via global channel
    pub(crate) async fn notify_update(&self, event: &Event) {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let filter = Filter::new()
            .ids(event.tags.event_ids().copied())
            .kind(Kind::FileMetadata);

        if let Err(e) = self
            .client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await
        {
            log::error!("Failed to subscribe for file metadata: {}", e);
        } else {
            self.global_sender
                .send(NostrSignal::AppUpdate(event.to_owned()))
                .await
                .ok();
        }
    }
}

fn is_first_run() -> Result<bool, anyhow::Error> {
    let flag = support_dir().join(".coop_first_run");

    if !flag.exists() {
        fs::write(&flag, "")?;
        Ok(true) // First run
    } else {
        Ok(false) // Not first run
    }
}
