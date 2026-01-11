use std::collections::HashSet;
use std::time::Duration;

use anyhow::Error;
use common::{config_dir, BOOTSTRAP_RELAYS, SEARCH_RELAYS};
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_lmdb::NostrLmdb;
use nostr_sdk::prelude::*;

mod device;
mod event;
mod gossip;
mod identity;

pub use device::*;
pub use event::*;
pub use gossip::*;
pub use identity::*;

use crate::identity::Identity;

pub fn init(cx: &mut App) {
    NostrRegistry::set_global(cx.new(NostrRegistry::new), cx);
}

/// Default timeout for subscription
pub const TIMEOUT: u64 = 3;

/// Default subscription id for gift wrap events
pub const GIFTWRAP_SUBSCRIPTION: &str = "giftwrap-events";

struct GlobalNostrRegistry(Entity<NostrRegistry>);

impl Global for GlobalNostrRegistry {}

/// Nostr Registry
#[derive(Debug)]
pub struct NostrRegistry {
    /// Nostr client
    client: Client,

    /// App keys
    ///
    /// Used for Nostr Connect and NIP-4e operations
    app_keys: Keys,

    /// Current identity (user's public key)
    ///
    /// Set by the current Nostr signer
    identity: Entity<Identity>,

    /// Gossip implementation
    gossip: Entity<Gossip>,

    /// Tasks for asynchronous operations
    tasks: Vec<Task<Result<(), Error>>>,

    /// Subscriptions
    _subscriptions: Vec<Subscription>,
}

impl NostrRegistry {
    /// Retrieve the global nostr state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalNostrRegistry>().0.clone()
    }

    /// Set the global nostr instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalNostrRegistry(state));
    }

    /// Create a new nostr instance
    fn new(cx: &mut Context<Self>) -> Self {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        // Construct the nostr client options
        let opts = ClientOptions::new()
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(600),
            });

        // Construct the lmdb
        let lmdb = cx.background_executor().block(async move {
            let path = config_dir().join("nostr");
            NostrLmdb::open(path)
                .await
                .expect("Failed to initialize database")
        });

        // Construct the nostr client
        let client = ClientBuilder::default().database(lmdb).opts(opts).build();
        let _ = tracker();

        // Get the app keys
        let app_keys = Self::create_or_init_app_keys().unwrap();

        // Construct the gossip entity
        let gossip = cx.new(|_| Gossip::default());
        let async_gossip = gossip.downgrade();

        // Construct the identity entity
        let identity = cx.new(|_| Identity::default());

        // Channel for communication between nostr and gpui
        let (tx, rx) = flume::bounded::<Event>(2048);

        let mut subscriptions = vec![];
        let mut tasks = vec![];

        subscriptions.push(
            // Observe the identity entity
            cx.observe(&identity, |this, state, cx| {
                if state.read(cx).has_public_key() {
                    match state.read(cx).relay_list_state() {
                        RelayState::Initial => {
                            this.get_relay_list(cx);
                        }
                        RelayState::Set => {
                            if state.read(cx).messaging_relays_state() == RelayState::Initial {
                                this.get_profile(cx);
                                this.get_messaging_relays(cx);
                            };
                        }
                        _ => {}
                    }
                }
            }),
        );

        tasks.push(
            // Handle nostr notifications
            cx.background_spawn({
                let client = client.clone();

                async move { Self::handle_notifications(&client, &tx).await }
            }),
        );

        tasks.push(
            // Update GPUI states
            cx.spawn(async move |_this, cx| {
                while let Ok(event) = rx.recv_async().await {
                    match event.kind {
                        Kind::RelayList => {
                            async_gossip.update(cx, |this, cx| {
                                this.insert_relays(&event);
                                cx.notify();
                            })?;
                        }
                        Kind::InboxRelays => {
                            async_gossip.update(cx, |this, cx| {
                                this.insert_messaging_relays(&event);
                                cx.notify();
                            })?;
                        }
                        _ => {}
                    }
                }

                Ok(())
            }),
        );

        Self {
            client,
            app_keys,
            identity,
            gossip,
            _subscriptions: subscriptions,
            tasks,
        }
    }

    /// Handle nostr notifications
    async fn handle_notifications(client: &Client, tx: &flume::Sender<Event>) -> Result<(), Error> {
        // Add bootstrap relay to the relay pool
        for url in BOOTSTRAP_RELAYS.into_iter() {
            client.add_relay(url).await?;
        }

        // Add search relay to the relay pool
        for url in SEARCH_RELAYS.into_iter() {
            client.add_relay(url).await?;
        }

        // Connect to all added relays
        client.connect().await;

        // Handle nostr notifications
        let mut notifications = client.notifications();
        let mut processed_events = HashSet::new();

        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Message { message, relay_url } = notification {
                match message {
                    RelayMessage::Event {
                        event,
                        subscription_id,
                    } => {
                        if !processed_events.insert(event.id) {
                            // Skip if the event has already been processed
                            continue;
                        }

                        match event.kind {
                            Kind::RelayList => {
                                // Automatically get messaging relays for each member when the user opens a room
                                if subscription_id.as_str().starts_with("room-") {
                                    Self::get_adv_events_by(client, event.as_ref()).await?;
                                }

                                tx.send_async(event.into_owned()).await?;
                            }
                            Kind::InboxRelays => {
                                tx.send_async(event.into_owned()).await?;
                            }
                            _ => {}
                        }
                    }
                    RelayMessage::Ok {
                        event_id, message, ..
                    } => {
                        let msg = MachineReadablePrefix::parse(&message);
                        let mut tracker = tracker().write().await;

                        // Handle authentication messages
                        if let Some(MachineReadablePrefix::AuthRequired) = msg {
                            // Keep track of events that need to be resent after authentication
                            tracker.add_to_pending(event_id, relay_url);
                        } else {
                            // Keep track of events sent by Coop
                            tracker.sent(event_id)
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Automatically get messaging relays and encryption announcement from a received relay list
    async fn get_adv_events_by(client: &Client, event: &Event) -> Result<(), Error> {
        // Subscription options
        let opts = SubscribeAutoCloseOptions::default()
            .timeout(Some(Duration::from_secs(TIMEOUT)))
            .exit_policy(ReqExitPolicy::ExitOnEOSE);

        // Extract write relays from event
        let write_relays: Vec<&RelayUrl> = nip65::extract_relay_list(event)
            .filter_map(|(url, metadata)| {
                if metadata.is_none() || metadata == &Some(RelayMetadata::Write) {
                    Some(url)
                } else {
                    None
                }
            })
            .collect();

        // Ensure relay connections
        for relay in write_relays.iter() {
            client.add_relay(*relay).await?;
            client.connect_relay(*relay).await?;
        }

        // Construct filter for inbox relays
        let inbox = Filter::new()
            .kind(Kind::InboxRelays)
            .author(event.pubkey)
            .limit(1);

        // Construct filter for encryption announcement
        let announcement = Filter::new()
            .kind(Kind::Custom(10044))
            .author(event.pubkey)
            .limit(1);

        client
            .subscribe_to(write_relays, vec![inbox, announcement], Some(opts))
            .await?;

        Ok(())
    }

    /// Get or create a new app keys
    fn create_or_init_app_keys() -> Result<Keys, Error> {
        let dir = config_dir().join(".app_keys");
        let content = match std::fs::read(&dir) {
            Ok(content) => content,
            Err(_) => {
                // Generate new keys if file doesn't exist
                let keys = Keys::generate();
                let secret_key = keys.secret_key();

                std::fs::create_dir_all(dir.parent().unwrap())?;
                std::fs::write(&dir, secret_key.to_secret_bytes())?;

                return Ok(keys);
            }
        };
        let secret_key = SecretKey::from_slice(&content)?;
        let keys = Keys::new(secret_key);

        Ok(keys)
    }

    /// Get the nostr client
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Get the app keys
    pub fn app_keys(&self) -> &Keys {
        &self.app_keys
    }

    /// Get current identity
    pub fn identity(&self) -> Entity<Identity> {
        self.identity.clone()
    }

    /// Get a relay hint (messaging relay) for a given public key
    pub fn relay_hint(&self, public_key: &PublicKey, cx: &App) -> Option<RelayUrl> {
        self.gossip
            .read(cx)
            .messaging_relays(public_key)
            .first()
            .cloned()
    }

    /// Get a list of write relays for a given public key
    pub fn write_relays(&self, public_key: &PublicKey, cx: &App) -> Task<Vec<RelayUrl>> {
        let client = self.client();
        let relays = self.gossip.read(cx).write_relays(public_key);

        cx.background_spawn(async move {
            // Ensure relay connections
            for url in relays.iter() {
                client.add_relay(url).await.ok();
                client.connect_relay(url).await.ok();
            }

            relays
        })
    }

    /// Get a list of read relays for a given public key
    pub fn read_relays(&self, public_key: &PublicKey, cx: &App) -> Task<Vec<RelayUrl>> {
        let client = self.client();
        let relays = self.gossip.read(cx).read_relays(public_key);

        cx.background_spawn(async move {
            // Ensure relay connections
            for url in relays.iter() {
                client.add_relay(url).await.ok();
                client.connect_relay(url).await.ok();
            }

            relays
        })
    }

    /// Get a list of messaging relays for a given public key
    pub fn messaging_relays(&self, public_key: &PublicKey, cx: &App) -> Task<Vec<RelayUrl>> {
        let client = self.client();
        let relays = self.gossip.read(cx).messaging_relays(public_key);

        cx.background_spawn(async move {
            // Ensure relay connections
            for url in relays.iter() {
                client.add_relay(url).await.ok();
                client.connect_relay(url).await.ok();
            }

            relays
        })
    }

    /// Set the signer for the nostr client and verify the public key
    pub fn set_signer<T>(&mut self, signer: T, cx: &mut Context<Self>)
    where
        T: NostrSigner + 'static,
    {
        let client = self.client();
        let identity = self.identity.downgrade();

        // Create a task to update the signer and verify the public key
        let task: Task<Result<PublicKey, Error>> = cx.background_spawn(async move {
            // Update signer
            client.set_signer(signer).await;

            // Verify signer
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            Ok(public_key)
        });

        self.tasks.push(cx.spawn(async move |_this, cx| {
            match task.await {
                Ok(public_key) => {
                    identity.update(cx, |this, cx| {
                        this.set_public_key(public_key);
                        cx.notify();
                    })?;
                }
                Err(e) => {
                    log::error!("Failed to set signer: {e}");
                }
            };

            Ok(())
        }));
    }

    /// Unset the current signer
    pub fn unset_signer(&mut self, cx: &mut Context<Self>) {
        let client = self.client();
        let async_identity = self.identity.downgrade();

        self.tasks.push(cx.spawn(async move |_this, cx| {
            // Unset the signer from nostr client
            cx.background_executor()
                .await_on_background(async move {
                    client.unset_signer().await;
                })
                .await;

            // Unset the current identity
            async_identity
                .update(cx, |this, cx| {
                    this.unset_public_key();
                    cx.notify();
                })
                .ok();

            Ok(())
        }));
    }

    // Get relay list for current user
    fn get_relay_list(&mut self, cx: &mut Context<Self>) {
        let client = self.client();
        let async_identity = self.identity.downgrade();
        let public_key = self.identity().read(cx).public_key();

        let task: Task<Result<RelayState, Error>> = cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::RelayList)
                .author(public_key)
                .limit(1);

            let mut stream = client
                .stream_events_from(BOOTSTRAP_RELAYS, vec![filter], Duration::from_secs(TIMEOUT))
                .await?;

            while let Some((_url, res)) = stream.next().await {
                match res {
                    Ok(event) => {
                        log::info!("Received relay list event: {event:?}");
                        return Ok(RelayState::Set);
                    }
                    Err(e) => {
                        log::error!("Failed to receive relay list event: {e}");
                    }
                }
            }

            Ok(RelayState::NotSet)
        });

        self.tasks.push(cx.spawn(async move |_this, cx| {
            match task.await {
                Ok(state) => {
                    async_identity
                        .update(cx, |this, cx| {
                            this.set_relay_list_state(state);
                            cx.notify();
                        })
                        .ok();
                }
                Err(e) => {
                    log::error!("Failed to get relay list: {e}");
                }
            }

            Ok(())
        }));
    }

    /// Get profile and contact list for current user
    fn get_profile(&mut self, cx: &mut Context<Self>) {
        let client = self.client();
        let public_key = self.identity().read(cx).public_key();
        let write_relays = self.write_relays(&public_key, cx);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let mut urls = vec![];
            urls.extend(write_relays.await);
            urls.extend(
                BOOTSTRAP_RELAYS
                    .iter()
                    .filter_map(|url| RelayUrl::parse(url).ok()),
            );

            // Construct subscription options
            let opts = SubscribeAutoCloseOptions::default()
                .exit_policy(ReqExitPolicy::ExitOnEOSE)
                .timeout(Some(Duration::from_secs(TIMEOUT)));

            // Filter for metadata
            let metadata = Filter::new()
                .kind(Kind::Metadata)
                .limit(1)
                .author(public_key);

            // Filter for contact list
            let contact_list = Filter::new()
                .kind(Kind::ContactList)
                .limit(1)
                .author(public_key);

            client
                .subscribe_to(urls, vec![metadata, contact_list], Some(opts))
                .await?;

            Ok(())
        });

        task.detach();
    }

    /// Get messaging relays for current user
    fn get_messaging_relays(&mut self, cx: &mut Context<Self>) {
        let client = self.client();
        let async_identity = self.identity.downgrade();
        let public_key = self.identity().read(cx).public_key();
        let write_relays = self.write_relays(&public_key, cx);

        let task: Task<Result<RelayState, Error>> = cx.background_spawn(async move {
            let urls = write_relays.await;

            // Construct the filter for inbox relays
            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            // Stream events from the write relays
            let mut stream = client
                .stream_events_from(urls, vec![filter], Duration::from_secs(TIMEOUT))
                .await?;

            while let Some((_url, res)) = stream.next().await {
                match res {
                    Ok(event) => {
                        log::info!("Received messaging relays event: {event:?}");
                        return Ok(RelayState::Set);
                    }
                    Err(e) => {
                        log::error!("Failed to get messaging relays: {e}");
                    }
                }
            }

            Ok(RelayState::NotSet)
        });

        self.tasks.push(cx.spawn(async move |_this, cx| {
            match task.await {
                Ok(state) => {
                    async_identity
                        .update(cx, |this, cx| {
                            this.set_messaging_relays_state(state);
                            cx.notify();
                        })
                        .ok();
                }
                Err(e) => {
                    log::error!("Failed to get messaging relays: {e}");
                }
            }

            Ok(())
        }));
    }
}
