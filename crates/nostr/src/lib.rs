use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Error};
pub use encryption::*;
use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_gossip_memory::prelude::*;
use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use smol::lock::Mutex;
use states::{config_dir, BOOTSTRAP_RELAYS, INBOX_SUB_ID, SEARCH_RELAYS};
pub use storage::*;
pub use tracker::*;

mod encryption;
mod storage;
mod tracker;

pub fn init(cx: &mut App) {
    NostrRegistry::set_global(cx.new(NostrRegistry::new), cx);
}

struct GlobalNostrRegistry(Entity<NostrRegistry>);

impl Global for GlobalNostrRegistry {}

/// Nostr Registry
#[derive(Debug)]
pub struct NostrRegistry {
    /// Nostr client instance
    client: Arc<Client>,

    /// Tracks activity related to Nostr events
    tracker: Arc<Mutex<EventTracker>>,

    /// Manages caching of nostr events
    cache_manager: Arc<Mutex<CacheManager>>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 1]>,
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

        let path = config_dir().join("nostr");
        let lmdb = NostrLMDB::open(path).expect("Failed to initialize database");
        let gossip = NostrGossipMemory::unbounded();

        // Nostr client options
        let opts = ClientOptions::new()
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(600),
            });

        // Construct the nostr client
        let client = Arc::new(
            ClientBuilder::default()
                .gossip(gossip)
                .database(lmdb)
                .opts(opts)
                .build(),
        );

        let tracker = Arc::new(Mutex::new(EventTracker::default()));
        let cache_manager = Arc::new(Mutex::new(CacheManager::default()));

        let mut tasks = smallvec![];

        tasks.push(
            // Establish connection to the bootstrap relays
            //
            // And handle notifications from the nostr relay pool channel
            cx.background_spawn({
                let client = Arc::clone(&client);
                let cache_manager = Arc::clone(&cache_manager);
                let tracker = Arc::clone(&tracker);

                let _ = processed_events();
                let _ = initialized_at();

                async move {
                    // Connect to the bootstrap relays
                    Self::connect(&client).await;

                    // Handle notifications from the relay pool
                    Self::handle_notifications(&client, &cache_manager, &tracker).await;
                }
            }),
        );

        Self {
            client,
            tracker,
            cache_manager,
            _tasks: tasks,
        }
    }

    /// Establish connection to the bootstrap relays
    async fn connect(client: &Client) {
        // Get all bootstrapping relays
        let mut urls = vec![];
        urls.extend(BOOTSTRAP_RELAYS);
        urls.extend(SEARCH_RELAYS);

        // Add relay to the relay pool
        for url in urls.into_iter() {
            client.add_relay(url).await.ok();
        }

        // Connect to all added relays
        client.connect().await;
    }

    async fn handle_notifications(
        client: &Client,
        cache: &Arc<Mutex<CacheManager>>,
        tracker: &Arc<Mutex<EventTracker>>,
    ) {
        let mut notifications = client.notifications();

        while let Ok(notification) = notifications.recv().await {
            let RelayPoolNotification::Message { message, relay_url } = notification else {
                // Skip if the notification is not a message
                continue;
            };

            match message {
                RelayMessage::Event { event, .. } => {
                    // Skip events that have already been processed
                    if !processed_events().lock().await.insert(event.id) {
                        continue;
                    }

                    match event.kind {
                        Kind::RelayList => {
                            if Self::is_self_authored(client, &event).await {
                                // Fetch user's metadata event
                                if let Err(e) =
                                    Self::subscribe(client, event.pubkey, Kind::Metadata).await
                                {
                                    log::error!("Failed to subscribe to metadata event: {e}");
                                }

                                // Fetch user's contact list event
                                if let Err(e) =
                                    Self::subscribe(client, event.pubkey, Kind::ContactList).await
                                {
                                    log::error!("Failed to subscribe to contact list event: {e}");
                                }

                                // Fetch user's messaging relays event
                                if let Err(e) =
                                    Self::subscribe(client, event.pubkey, Kind::InboxRelays).await
                                {
                                    log::error!("Failed to subscribe to relay event: {e}");
                                }
                            }
                        }
                        Kind::InboxRelays => {
                            // Extract up to 3 messaging relays
                            let urls: Vec<RelayUrl> =
                                nip17::extract_relay_list(&event).take(3).cloned().collect();

                            // Cache the messaging relays
                            cache.lock().await.insert_relay(event.pubkey, urls);

                            // Subscribe to gift wrap events if event is from current user
                            if Self::is_self_authored(client, &event).await {
                                if let Err(e) = Self::get_messages(client, event.pubkey).await {
                                    log::error!("Failed to subscribe to gift wrap events: {e}");
                                }
                            }
                        }
                        Kind::ContactList => {
                            if Self::is_self_authored(client, &event).await {
                                let pubkeys: Vec<_> = event.tags.public_keys().copied().collect();

                                if let Err(e) = Self::get_metadata_for_list(client, pubkeys).await {
                                    log::error!("Failed to get metadata for list: {e}");
                                }
                            }
                        }
                        _ => {}
                    };
                }
                RelayMessage::Ok {
                    event_id, message, ..
                } => {
                    let msg = MachineReadablePrefix::parse(&message);
                    let mut tracker = tracker.lock().await;

                    // Message that need to be authenticated will be handled separately
                    if let Some(MachineReadablePrefix::AuthRequired) = msg {
                        // Keep track of events that need to be resent after authentication
                        tracker.resend_queue.insert(event_id, relay_url);
                    } else {
                        // Keep track of events sent by Coop
                        tracker.sent_ids.insert(event_id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if event is published by current user
    async fn is_self_authored(client: &Client, event: &Event) -> bool {
        if let Ok(signer) = client.signer().await {
            if let Ok(public_key) = signer.get_public_key().await {
                return public_key == event.pubkey;
            }
        }
        false
    }

    /// Subscribe for events that match the given kind for a given author
    async fn subscribe(client: &Client, author: PublicKey, kind: Kind) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let filter = Filter::new().author(author).kind(kind).limit(1);

        // Subscribe to filters from the user's write relays
        client.subscribe(filter, Some(opts)).await?;

        Ok(())
    }

    /// Get all gift wrap events in the messaging relays for a given public key
    async fn get_messages(client: &Client, public_key: PublicKey) -> Result<(), Error> {
        let id = SubscriptionId::new(INBOX_SUB_ID);
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        // Subscribe to filters to user's messaging relays
        client.subscribe_with_id(id, filter, None).await?;

        Ok(())
    }

    /// Get metadata for a list of public keys
    async fn get_metadata_for_list(client: &Client, pubkeys: Vec<PublicKey>) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];

        // Return if the list is empty
        if pubkeys.is_empty() {
            return Err(anyhow!("You need at least one public key".to_string(),));
        }

        let filter = Filter::new()
            .limit(pubkeys.len() * kinds.len() + 10)
            .authors(pubkeys)
            .kinds(kinds);

        // Subscribe to filters to the bootstrap relays
        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    /// Returns a reference to the nostr client.
    pub fn client(&self) -> Arc<Client> {
        Arc::clone(&self.client)
    }

    /// Returns a reference to the event tracker.
    pub fn tracker(&self) -> Arc<Mutex<EventTracker>> {
        Arc::clone(&self.tracker)
    }

    /// Returns a reference to the cache manager.
    pub fn cache_manager(&self) -> Arc<Mutex<CacheManager>> {
        Arc::clone(&self.cache_manager)
    }
}
