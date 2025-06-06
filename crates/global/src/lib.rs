use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Error};
use constants::{
    ALL_MESSAGES_SUB_ID, APP_ID, APP_PUBKEY, BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT,
    METADATA_BATCH_TIMEOUT, NEW_MESSAGE_SUB_ID, SEARCH_RELAYS,
};
use nostr_keyring::prelude::*;
use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::lock::RwLock;

use crate::constants::{KEYRING_PATH, NIP17_RELAYS, NIP65_RELAYS};

pub mod constants;
pub mod paths;

static GLOBALS: OnceLock<Globals> = OnceLock::new();

#[derive(Debug)]
pub enum NostrSignal {
    SignerUpdated,
    /// Receive event
    Event(Event),
    /// Receive app update
    AppUpdate(Event),
    /// Receive EOSE
    Eose,
}

pub struct Globals {
    /// The Nostr SDK client
    pub client: Client,
    /// TODO: add document
    pub client_signer: Keys,
    /// TODO: add document,
    pub identity: Arc<RwLock<Option<Profile>>>,
    /// TODO: add document
    pub auto_close: Option<SubscribeAutoCloseOptions>,
    /// TODO: add document
    pub global_sender: smol::channel::Sender<NostrSignal>,
    /// TODO: add document
    pub global_receiver: smol::channel::Receiver<NostrSignal>,
    /// TODO: add document
    pub batch_sender: smol::channel::Sender<Vec<PublicKey>>,
    /// TODO: add document
    pub batch_receiver: smol::channel::Receiver<Vec<PublicKey>>,
    /// TODO: add document
    pub persons: RwLock<BTreeMap<PublicKey, Option<Metadata>>>,
}

pub fn shared_state() -> &'static Globals {
    GLOBALS.get_or_init(|| {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let keyring = NostrKeyring::new(KEYRING_PATH);
        // Get the client signer or generate a new one if it doesn't exist
        let client_signer = if let Ok(keys) = keyring.get("client") {
            keys
        } else {
            Keys::generate()
        };

        let opts = Options::new().gossip(true);
        let lmdb = NostrLMDB::open(nostr_file()).expect("Database is NOT initialized");

        let (global_sender, global_receiver) = smol::channel::bounded::<NostrSignal>(2048);
        let (batch_sender, batch_receiver) = smol::channel::bounded::<Vec<PublicKey>>(2048);

        Globals {
            client: ClientBuilder::default().database(lmdb).opts(opts).build(),
            persons: RwLock::new(BTreeMap::new()),
            auto_close: Some(
                SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE),
            ),
            identity: Arc::new(RwLock::new(None)),
            client_signer,
            global_sender,
            global_receiver,
            batch_sender,
            batch_receiver,
        }
    })
}

impl Globals {
    pub async fn start(&self) {
        self.connect().await;
        self.subscribe_for_app_updates().await;

        nostr_sdk::async_utility::task::spawn(async move {
            let mut batch: BTreeSet<PublicKey> = BTreeSet::new();
            let timeout_duration = Duration::from_millis(METADATA_BATCH_TIMEOUT);

            loop {
                let timeout = smol::Timer::after(timeout_duration);

                enum BatchEvent {
                    NewKeys(Vec<PublicKey>),
                    Timeout,
                    ChannelClosed,
                }

                let event = smol::future::or(
                    async {
                        match shared_state().batch_receiver.recv().await {
                            Ok(public_keys) => BatchEvent::NewKeys(public_keys),
                            Err(_) => BatchEvent::ChannelClosed,
                        }
                    },
                    async {
                        timeout.await;
                        BatchEvent::Timeout
                    },
                )
                .await;

                match event {
                    BatchEvent::NewKeys(public_keys) => {
                        batch.extend(public_keys);

                        // Process immediately if batch limit reached
                        if batch.len() >= METADATA_BATCH_LIMIT {
                            shared_state()
                                .sync_data_for_pubkeys(mem::take(&mut batch))
                                .await;
                        }
                    }
                    BatchEvent::Timeout => {
                        // Process current batch if not empty
                        if !batch.is_empty() {
                            shared_state()
                                .sync_data_for_pubkeys(mem::take(&mut batch))
                                .await;
                        }
                    }
                    BatchEvent::ChannelClosed => {
                        // Process remaining batch and exit
                        if !batch.is_empty() {
                            shared_state().sync_data_for_pubkeys(batch).await;
                        }
                        break;
                    }
                }
            }
        });

        let mut notifications = self.client.notifications();
        let mut processed_events: BTreeSet<EventId> = BTreeSet::new();

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
                                self.unwrap_event(&subscription_id, &event).await;
                            }
                            Kind::Metadata => {
                                self.insert_person(&event).await;
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
                        if *subscription_id == SubscriptionId::new(ALL_MESSAGES_SUB_ID) {
                            self.global_sender.send(NostrSignal::Eose).await.ok();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub async fn set_signer<S>(&self, signer: S) -> Result<(), Error>
    where
        S: NostrSigner + 'static,
    {
        let public_key = signer.get_public_key().await?;

        // Update signer
        self.client.set_signer(signer).await;

        // Fetch user's metadata
        let metadata = shared_state()
            .client
            .fetch_metadata(public_key, Duration::from_secs(2))
            .await?
            .unwrap_or_default();

        let profile = Profile::new(public_key, metadata);
        let mut guard = self.identity.write().await;

        // Update the identity
        *guard = Some(profile);

        // Notify GPUi via the global channel
        self.global_sender.send(NostrSignal::SignerUpdated).await?;

        // Subscribe
        self.subscribe_for_user_data().await;

        Ok(())
    }

    pub async fn new_account(&self, metadata: Metadata) {
        let keys = Keys::generate();
        let profile = Profile::new(keys.public_key(), metadata.clone());

        // Update signer
        self.client.set_signer(keys).await;

        // Set metadata
        self.client.set_metadata(&metadata).await.ok();

        // Create relay list
        let builder = EventBuilder::new(Kind::RelayList, "").tags(
            NIP65_RELAYS.into_iter().filter_map(|url| {
                if let Ok(url) = RelayUrl::parse(url) {
                    Some(Tag::relay_metadata(url, None))
                } else {
                    None
                }
            }),
        );

        if let Err(e) = self.client.send_event_builder(builder).await {
            log::error!("Failed to send relay list event: {}", e);
        };

        // Create messaging relay list
        let builder = EventBuilder::new(Kind::InboxRelays, "").tags(
            NIP17_RELAYS.into_iter().filter_map(|url| {
                if let Ok(url) = RelayUrl::parse(url) {
                    Some(Tag::relay(url))
                } else {
                    None
                }
            }),
        );

        if let Err(e) = self.client.send_event_builder(builder).await {
            log::error!("Failed to send messaging relay list event: {}", e);
        };

        let mut guard = self.identity.write().await;

        // Update the identity
        *guard = Some(profile);

        // Notify GPUi via the global channel
        self.global_sender
            .send(NostrSignal::SignerUpdated)
            .await
            .ok();

        // Subscribe
        self.subscribe_for_user_data().await;
    }

    pub fn identity(&self) -> Option<Profile> {
        self.identity.read_blocking().as_ref().cloned()
    }

    pub async fn async_identity(&self) -> Option<Profile> {
        self.identity.read().await.as_ref().cloned()
    }

    pub fn person(&self, public_key: &PublicKey) -> Profile {
        let metadata = if let Some(metadata) = self.persons.read_blocking().get(public_key) {
            metadata.clone().unwrap_or_default()
        } else {
            Metadata::default()
        };

        Profile::new(*public_key, metadata)
    }

    pub async fn async_person(&self, public_key: &PublicKey) -> Profile {
        let metadata = if let Some(metadata) = self.persons.read().await.get(public_key) {
            metadata.clone().unwrap_or_default()
        } else {
            Metadata::default()
        };

        Profile::new(*public_key, metadata)
    }

    async fn connect(&self) {
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

    async fn subscribe_for_user_data(&self) {
        let Some(profile) = self.identity.read().await.clone() else {
            return;
        };

        let public_key = profile.public_key();

        let metadata = Filter::new()
            .kinds(vec![
                Kind::Metadata,
                Kind::ContactList,
                Kind::InboxRelays,
                Kind::MuteList,
                Kind::SimpleGroups,
            ])
            .author(public_key)
            .limit(10);

        let data = Filter::new()
            .author(public_key)
            .kinds(vec![
                Kind::Metadata,
                Kind::ContactList,
                Kind::MuteList,
                Kind::SimpleGroups,
                Kind::InboxRelays,
                Kind::RelayList,
            ])
            .since(Timestamp::now());

        let msg = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
        let new_msg = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(public_key)
            .limit(0);

        let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
        let new_messages_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

        let client = &shared_state().client;
        let opts = shared_state().auto_close;

        client.subscribe(data, None).await.ok();

        client
            .subscribe(metadata, shared_state().auto_close)
            .await
            .ok();

        client
            .subscribe_with_id(all_messages_sub_id, msg, opts)
            .await
            .ok();

        client
            .subscribe_with_id(new_messages_sub_id, new_msg, None)
            .await
            .ok();

        log::info!("Subscribing to user's metadata...");
    }

    async fn subscribe_for_app_updates(&self) {
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
            .subscribe_to(BOOTSTRAP_RELAYS, filter, shared_state().auto_close)
            .await
        {
            log::error!("Failed to subscribe for app updates: {}", e);
        }

        log::info!("Subscribing to app updates...");
    }

    async fn set_unwrapped(&self, root: EventId, event: &Event, keys: &Keys) -> Result<(), Error> {
        // Must be use the random generated keys to sign this event
        let event = EventBuilder::new(Kind::Custom(30078), event.as_json())
            .tags(vec![Tag::identifier(root), Tag::event(root)])
            .sign(keys)
            .await?;

        // Only save this event into the local database
        self.client.database().save_event(&event).await?;

        Ok(())
    }

    async fn get_unwrapped(&self, target: EventId) -> Result<Event, Error> {
        let filter = Filter::new()
            .kind(Kind::Custom(30078))
            .event(target)
            .limit(1);

        if let Some(event) = self.client.database().query(filter).await?.first_owned() {
            Ok(Event::from_json(event.content)?)
        } else {
            Err(anyhow!("Event not found"))
        }
    }

    async fn unwrap_event(&self, sub_id: &SubscriptionId, event: &Event) {
        let new_messages_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
        let random_keys = Keys::generate();

        let event = match self.get_unwrapped(event.id).await {
            Ok(event) => event,
            Err(_) => match self.client.unwrap_gift_wrap(event).await {
                Ok(unwrap) => match unwrap.rumor.sign_with_keys(&random_keys) {
                    Ok(unwrapped) => {
                        self.set_unwrapped(event.id, &unwrapped, &random_keys)
                            .await
                            .ok();
                        unwrapped
                    }
                    Err(_) => return,
                },
                Err(_) => return,
            },
        };

        let mut pubkeys = vec![];
        pubkeys.extend(event.tags.public_keys());
        pubkeys.push(event.pubkey);

        // Send all pubkeys to the batch to sync metadata
        self.batch_sender.send(pubkeys).await.ok();

        // Save the event to the database, use for query directly.
        self.client.database().save_event(&event).await.ok();

        // Send this event to the GPUI
        if sub_id == &new_messages_id {
            self.global_sender
                .send(NostrSignal::Event(event))
                .await
                .ok();
        }
    }

    async fn extract_pubkeys_and_sync(&self, event: &Event) {
        if let Ok(signer) = self.client.signer().await {
            if let Ok(public_key) = signer.get_public_key().await {
                if public_key == event.pubkey {
                    let pubkeys = event.tags.public_keys().copied().collect::<Vec<_>>();
                    self.batch_sender.send(pubkeys).await.ok();
                }
            }
        }
    }

    async fn sync_data_for_pubkeys(&self, buffer: BTreeSet<PublicKey>) {
        let kinds = vec![
            Kind::Metadata,
            Kind::ContactList,
            Kind::InboxRelays,
            Kind::UserStatus,
        ];
        let filter = Filter::new()
            .limit(buffer.len() * kinds.len())
            .authors(buffer)
            .kinds(kinds);

        if let Err(e) = shared_state()
            .client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, shared_state().auto_close)
            .await
        {
            log::error!("Failed to sync metadata: {e}");
        }
    }

    async fn insert_person(&self, event: &Event) {
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

    async fn notify_update(&self, event: &Event) {
        let filter = Filter::new()
            .ids(event.tags.event_ids().copied())
            .kind(Kind::FileMetadata);

        if let Err(e) = self
            .client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, self.auto_close)
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
