use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Error};
use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use smol::lock::RwLock;

use crate::app_name;
use crate::constants::{
    BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT, METADATA_BATCH_TIMEOUT, QUERY_TIMEOUT, SEARCH_RELAYS,
};
use crate::paths::config_dir;
use crate::state::device::Device;
use crate::state::ingester::Ingester;
use crate::state::tracker::EventTracker;

mod device;
mod ingester;
mod signal;
mod tracker;

pub use signal::*;

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

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub device: RwLock<Device>,

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
        let device = RwLock::new(Device::default());
        let event_tracker = RwLock::new(EventTracker::default());

        let signal = Signal::default();
        let ingester = Ingester::default();

        Self {
            client,
            device,
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

    /// Returns a reference to the device
    pub fn device(&'static self) -> &'static RwLock<Device> {
        &self.device
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

                    // Initialize client keys
                    self.init_client_keys().await.ok();

                    // Exit the loop
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
                                if let Ok(announcement) = self.extract_announcement(&event) {
                                    self.signal
                                        .send(SignalKind::EncryptionSet(announcement))
                                        .await;
                                }
                            }
                        }
                        // Encryption Keys request event
                        Kind::Custom(4454) => {
                            if let Ok(true) = self.is_self_authored(&event).await {
                                if let Ok(announcement) = self.extract_announcement(&event) {
                                    self.signal
                                        .send(SignalKind::EncryptionRequest(announcement))
                                        .await;
                                }
                            }
                        }
                        // Encryption Keys response event
                        Kind::Custom(4455) => {
                            if let Ok(true) = self.is_self_authored(&event).await {
                                if let Ok(response) = self.extract_response(&event) {
                                    self.signal
                                        .send(SignalKind::EncryptionResponse(response))
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
                                if let Err(e) = self.get_announcement(author).await {
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
                            if let Err(e) = self.extract_rumor(&event).await {
                                log::error!("Failed to extract rumor: {e}");
                            }
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
                    let mut tracker = self.event_tracker.write().await;

                    // Keep track of events sent by Coop
                    tracker.sent_ids.insert(event_id);

                    // Keep track of events that need to be resend after auth
                    if let Some(MachineReadablePrefix::AuthRequired) = msg {
                        tracker.resend_queue.insert(event_id, relay_url);
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

    /// Encrypt and store a key in the local database.
    pub async fn set_keys(&self, kind: impl Into<String>, value: String) -> Result<(), Error> {
        let signer = self.client.signer().await?;
        let public_key = signer.get_public_key().await?;

        // Encrypt the value
        let content = signer.nip44_encrypt(&public_key, value.as_ref()).await?;

        // Construct the application data event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
            .tag(Tag::identifier(format!("coop:{}", kind.into())))
            .build(public_key)
            .sign(&Keys::generate())
            .await?;

        // Save the event to the database
        self.client.database().save_event(&event).await?;

        Ok(())
    }

    /// Get and decrypt a key from the local database.
    pub async fn get_keys(&self, kind: impl Into<String>) -> Result<Keys, Error> {
        let signer = self.client.signer().await?;
        let public_key = signer.get_public_key().await?;

        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(format!("coop:{}", kind.into()));

        if let Some(event) = self.client.database().query(filter).await?.first() {
            let content = signer.nip44_decrypt(&public_key, &event.content).await?;
            let secret = SecretKey::parse(&content)?;
            let keys = Keys::new(secret);

            Ok(keys)
        } else {
            Err(anyhow!("Key not found"))
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
        let timeout = Duration::from_secs(QUERY_TIMEOUT);
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

    /// Initialize the client keys to communicate between clients
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn init_client_keys(&self) -> Result<(), Error> {
        // Get the keys from the database or generate new ones
        let keys = self
            .get_keys("client")
            .await
            .unwrap_or_else(|_| Keys::generate());

        // Initialize the client keys
        let mut device = self.device.write().await;
        device.client_keys = Some(Arc::new(keys));

        Ok(())
    }

    /// Get and verify encryption announcement for a given public key
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn get_announcement(&self, public_key: PublicKey) -> Result<(), Error> {
        let timeout = Duration::from_secs(QUERY_TIMEOUT);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new()
            .kind(Kind::Custom(10044))
            .author(public_key)
            .limit(1);

        // Subscribe to events from user's nip65 relays
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

    /// Generate encryption keys and announce them
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn init_encryption_keys(&self) -> Result<(), Error> {
        let signer = self.client.signer().await?;
        let keys = Keys::generate();
        let public_key = keys.public_key();
        let secret = keys.secret_key().to_secret_hex();

        // Initialize the encryption keys
        let mut device = self.device.write().await;
        device.encryption_keys = Some(Arc::new(keys));

        // Store the encryption keys for future use
        self.set_keys("encryption", secret).await?;

        // Construct the announcement event
        let event = EventBuilder::new(Kind::Custom(10044), "")
            .tags(vec![
                Tag::client(app_name()),
                Tag::custom(TagKind::custom("n"), vec![public_key]),
            ])
            .sign(&signer)
            .await?;

        // Send the announcement event to the relays
        self.client.send_event(&event).await?;

        Ok(())
    }

    /// User has previously set encryption keys, load them from storage
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn load_encryption_keys(&self, announcement: &Announcement) -> Result<(), Error> {
        let keys = self.get_keys("encryption").await?;

        // Check if the encryption keys match the announcement
        if announcement.public_key() == keys.public_key() {
            let mut device = self.device.write().await;
            device.encryption_keys = Some(Arc::new(keys));

            Ok(())
        } else {
            Err(anyhow!("Not found"))
        }
    }

    /// Request encryption keys from other clients
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn request_encryption_keys(&self) -> Result<bool, Error> {
        let mut wait_for_approval = false;
        let device = self.device.read().await;

        // Client Keys are always known at this point
        let Some(client_keys) = device.client_keys.as_ref() else {
            return Err(anyhow!("Client Keys is required"));
        };

        let signer = self.client.signer().await?;
        let public_key = signer.get_public_key().await?;
        let client_pubkey = client_keys.get_public_key().await?;

        // Get the encryption keys response from the database first
        let filter = Filter::new()
            .kind(Kind::Custom(4455))
            .author(public_key)
            .pubkey(client_pubkey)
            .limit(1);

        match self.client.database().query(filter).await?.first_owned() {
            // Found encryption keys that shared by other clients
            Some(event) => {
                let root_device = event
                    .tags
                    .find(TagKind::custom("P"))
                    .and_then(|tag| tag.content())
                    .and_then(|content| PublicKey::parse(content).ok())
                    .context("Invalid event's tags")?;

                let payload = event.content.as_str();
                let decrypted = client_keys.nip44_decrypt(&root_device, payload).await?;

                let secret = SecretKey::from_hex(&decrypted)?;
                let keys = Keys::new(secret);

                // No longer need to hold the reader for device
                drop(device);

                let mut device = self.device.write().await;
                device.encryption_keys = Some(Arc::new(keys));
            }
            None => {
                // Construct encryption keys request event
                let event = EventBuilder::new(Kind::Custom(4454), "")
                    .tags(vec![
                        Tag::client(app_name()),
                        Tag::custom(TagKind::custom("pubkey"), vec![client_pubkey]),
                    ])
                    .sign(&signer)
                    .await?;

                // Send a request for encryption keys from other devices
                self.client.send_event(&event).await?;

                // Create a unique ID to control the subscription later
                let subscription_id = SubscriptionId::new("request");

                let filter = Filter::new()
                    .kind(Kind::Custom(4455))
                    .author(public_key)
                    .pubkey(client_pubkey)
                    .since(Timestamp::now());

                // Subscribe to the approval response event
                self.client
                    .subscribe_with_id(subscription_id, filter, None)
                    .await?;

                wait_for_approval = true;
            }
        }

        Ok(wait_for_approval)
    }

    /// Receive the encryption keys from other clients
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn receive_encryption_keys(&self, res: Response) -> Result<(), Error> {
        let device = self.device.read().await;

        // Client Keys are always known at this point
        let Some(client_keys) = device.client_keys.as_ref() else {
            return Err(anyhow!("Client Keys is required"));
        };

        let public_key = res.public_key();
        let payload = res.payload();

        // Decrypt the payload using the client keys
        let decrypted = client_keys.nip44_decrypt(&public_key, payload).await?;
        let secret = SecretKey::parse(&decrypted)?;
        let keys = Keys::new(secret);

        // No longer need to hold the reader for device
        drop(device);

        let mut device = self.device.write().await;
        device.encryption_keys = Some(Arc::new(keys));

        Ok(())
    }

    /// Response the encryption keys request from other clients
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub async fn response_encryption_keys(&self, target: PublicKey) -> Result<(), Error> {
        let device = self.device.read().await;

        // Client Keys are always known at this point
        let Some(client_keys) = device.client_keys.as_ref() else {
            return Err(anyhow!("Client Keys is required"));
        };

        let encryption = self.get_keys("encryption").await?;
        let client_pubkey = client_keys.get_public_key().await?;

        // Encrypt the encryption keys with the client's signer
        let payload = client_keys
            .nip44_encrypt(&target, &encryption.secret_key().to_secret_hex())
            .await?;

        // Construct the response event
        //
        // P tag: the current client's public key
        // p tag: the requester's public key
        let event = EventBuilder::new(Kind::Custom(4455), payload)
            .tags(vec![
                Tag::custom(TagKind::custom("P"), vec![client_pubkey]),
                Tag::public_key(target),
            ])
            .sign(client_keys)
            .await?;

        // Get the current user's signer and public key
        let signer = self.client.signer().await?;
        let public_key = signer.get_public_key().await?;

        // Get the current user's relay list
        let urls: Vec<RelayUrl> = self
            .client
            .database()
            .relay_list(public_key)
            .await?
            .into_iter()
            .filter_map(|(url, metadata)| {
                if metadata.is_none() || metadata == Some(RelayMetadata::Read) {
                    Some(url)
                } else {
                    None
                }
            })
            .collect();

        // Send the response event to the user's relay list
        self.client.send_event_to(urls, &event).await?;

        Ok(())
    }

    /// Get and verify NIP-17 relays for a given public key
    pub async fn get_nip17(&self, public_key: PublicKey) -> Result<(), Error> {
        let timeout = Duration::from_secs(QUERY_TIMEOUT);
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

    /// Gets messaging relays for public key
    pub async fn messaging_relays(&self, public_key: PublicKey) -> Vec<RelayUrl> {
        let mut relay_urls = vec![];

        let filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(public_key)
            .limit(1);

        if let Ok(events) = self.client.database().query(filter).await {
            if let Some(event) = events.first_owned() {
                let urls: Vec<RelayUrl> = nip17::extract_owned_relay_list(event).collect();

                // Connect to relays
                for url in urls.iter() {
                    self.client.add_relay(url).await.ok();
                    self.client.connect_relay(url).await.ok();
                }

                relay_urls.extend(urls.into_iter().take(3));
            }
        }

        relay_urls
    }

    /// Re-subscribes to gift wrap events
    pub async fn resubscribe_messages(&self) -> Result<(), Error> {
        let signer = self.client.signer().await?;
        let public_key = signer.get_public_key().await?;
        let urls = self.messaging_relays(public_key).await;

        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
        let id = SubscriptionId::new("inbox");

        // Unsubscribe the previous subscription
        self.client.unsubscribe(&id).await;

        // Subscribe to gift wrap events
        self.client
            .subscribe_with_id_to(urls, id, filter, None)
            .await?;

        Ok(())
    }

    /// Stores an unwrapped event in local database with reference to original
    async fn set_rumor(&self, id: EventId, rumor: &UnsignedEvent) -> Result<(), Error> {
        let rumor_id = rumor.id.context("Rumor is missing an event id")?;
        let author = rumor.pubkey;
        let conversation = self.conversation_id(rumor).to_string();

        let mut tags = rumor.tags.clone().to_vec();

        // Add a unique identifier
        tags.push(Tag::identifier(id));

        // Add a reference to the rumor's author
        tags.push(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)),
            [author],
        ));

        // Add a conversation id
        tags.push(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::C)),
            [conversation],
        ));

        // Add a reference to the rumor's id
        tags.push(Tag::event(rumor_id));

        // Add references to the rumor's participants
        for receiver in rumor.tags.public_keys().copied() {
            tags.push(Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                [receiver],
            ));
        }

        // Convert rumor to json
        let content = rumor.as_json();

        let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
            .tags(tags)
            .sign(&Keys::generate())
            .await?;

        self.client.database().save_event(&event).await?;

        Ok(())
    }

    /// Retrieves a previously unwrapped event from local database
    async fn get_rumor(&self, id: EventId) -> Result<UnsignedEvent, Error> {
        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(id)
            .limit(1);

        if let Some(event) = self.client.database().query(filter).await?.first_owned() {
            UnsignedEvent::from_json(event.content).map_err(|e| anyhow!(e))
        } else {
            Err(anyhow!("Event is not cached yet."))
        }
    }

    // Unwraps a gift-wrapped event and processes its contents.
    async fn extract_rumor(&self, gift_wrap: &Event) -> Result<(), Error> {
        // Try to get cached rumor first
        if let Ok(event) = self.get_rumor(gift_wrap.id).await {
            self.process_rumor(gift_wrap.id, event).await?;
            return Ok(());
        }

        // Try to unwrap with the available signer
        if let Ok(unwrapped) = self.try_unwrap_gift_wrap(gift_wrap).await {
            let sender = unwrapped.sender;
            let mut rumor_unsigned = unwrapped.rumor;

            if !self.verify_rumor_sender(sender, &rumor_unsigned) {
                return Err(anyhow!("Invalid rumor"));
            };

            // Generate event id for the rumor if it doesn't have one
            rumor_unsigned.ensure_id();

            self.set_rumor(gift_wrap.id, &rumor_unsigned).await?;
            self.process_rumor(gift_wrap.id, rumor_unsigned).await?;

            return Ok(());
        }

        Ok(())
    }

    // Helper method to try unwrapping with different signers
    async fn try_unwrap_gift_wrap(&self, gift_wrap: &Event) -> Result<UnwrappedGift, Error> {
        // Try to unwrap with the device's encryption keys first
        // NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
        if let Some(signer) = self.device.read().await.encryption_keys.as_ref() {
            if let Ok(unwrapped) = UnwrappedGift::from_gift_wrap(signer, gift_wrap).await {
                return Ok(unwrapped);
            }
        }

        // Get user's signer
        let signer = self.client.signer().await?;

        // Try to unwrap with the user's signer
        if let Ok(unwrapped) = UnwrappedGift::from_gift_wrap(&signer, gift_wrap).await {
            return Ok(unwrapped);
        }

        Err(anyhow!("No signer available"))
    }

    /// Process a rumor event.
    async fn process_rumor(&self, id: EventId, event: UnsignedEvent) -> Result<(), Error> {
        // Send all pubkeys to the metadata batch to sync data
        for public_key in event.tags.public_keys().copied() {
            self.ingester.send(public_key).await;
        }

        match event.created_at >= self.initialized_at {
            // New message: send a signal to notify the UI
            true => {
                self.signal.send(SignalKind::NewMessage((id, event))).await;
            }
            // Old message: Coop is probably processing the user's messages during initial load
            false => {
                self.gift_wrap_processing.store(true, Ordering::Release);
            }
        }

        Ok(())
    }

    /// Get the conversation ID for a given rumor (message).
    fn conversation_id(&self, rumor: &UnsignedEvent) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut pubkeys: Vec<PublicKey> = rumor.tags.public_keys().copied().collect();
        pubkeys.push(rumor.pubkey);
        pubkeys.sort();
        pubkeys.dedup();
        pubkeys.hash(&mut hasher);

        hasher.finish()
    }

    /// Verify that the sender of a rumor is the same as the sender of the event.
    fn verify_rumor_sender(&self, sender: PublicKey, rumor: &UnsignedEvent) -> bool {
        rumor.pubkey == sender
    }

    /// Extract an encryption keys announcement from an event.
    fn extract_announcement(&self, event: &Event) -> Result<Announcement, Error> {
        let public_key = event
            .tags
            .iter()
            .find(|tag| tag.kind().as_str() == "n" || tag.kind().as_str() == "pubkey")
            .and_then(|tag| tag.content())
            .and_then(|c| PublicKey::parse(c).ok())
            .context("Cannot parse public key from the event's tags")?;

        let client_name = event
            .tags
            .find(TagKind::Client)
            .and_then(|tag| tag.content())
            .map(|c| c.to_string())
            .context("Cannot parse client name from the event's tags")?;

        Ok(Announcement::new(event.id, client_name, public_key))
    }

    /// Extract an encryption keys response from an event.
    fn extract_response(&self, event: &Event) -> Result<Response, Error> {
        let payload = event.content.clone();
        let root_device = event
            .tags
            .find(TagKind::custom("P"))
            .and_then(|tag| tag.content())
            .and_then(|c| PublicKey::parse(c).ok())
            .context("Cannot parse public key from the event's tags")?;

        Ok(Response::new(payload, root_device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state;

    #[test]
    fn verify_rumor_sender_accepts_matching_sender() {
        let state = app_state();

        let keys = Keys::generate();
        let public_key = keys.public_key();
        let rumor = EventBuilder::text_note("hello").build(public_key);

        assert!(state.verify_rumor_sender(public_key, &rumor));
    }

    #[test]
    fn verify_rumor_sender_rejects_mismatched_sender() {
        let state = app_state();

        let sender_keys = Keys::generate();
        let rumor_keys = Keys::generate();
        let rumor = EventBuilder::text_note("spoof").build(rumor_keys.public_key());

        assert!(!state.verify_rumor_sender(sender_keys.public_key(), &rumor));
    }
}
