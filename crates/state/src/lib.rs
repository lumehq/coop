use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::{config_dir, BOOTSTRAP_RELAYS, SEARCH_RELAYS};
use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_gossip_memory::prelude::*;
use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use smol::lock::RwLock;
pub use storage::*;
pub use tracker::*;

mod storage;
mod tracker;

pub const GIFTWRAP_SUBSCRIPTION: &str = "default-inbox";
pub const ENCRYPTION_GIFTWARP_SUBSCRIPTION: &str = "encryption-inbox";

pub fn init(cx: &mut App) {
    NostrRegistry::set_global(cx.new(NostrRegistry::new), cx);
}

struct GlobalNostrRegistry(Entity<NostrRegistry>);

impl Global for GlobalNostrRegistry {}

/// Nostr Registry
#[derive(Debug)]
pub struct NostrRegistry {
    /// Nostr client instance
    client: Client,

    /// Tracks activity related to Nostr events
    tracker: Arc<RwLock<EventTracker>>,

    /// Manages caching of nostr events
    cache_manager: Arc<RwLock<CacheManager>>,

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

        // Construct the nostr client options
        let opts = ClientOptions::new()
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(600),
            });

        // Construct the nostr client
        let path = config_dir().join("nostr");
        let lmdb = NostrLMDB::open(path).expect("Failed to initialize database");
        let client = ClientBuilder::default().database(lmdb).opts(opts).build();

        let tracker = Arc::new(RwLock::new(EventTracker::default()));
        let cache = Arc::new(RwLock::new(CacheManager::default()));

        let mut tasks = smallvec![];

        tasks.push(
            // Establish connection to the bootstrap relays
            //
            // And handle notifications from the nostr relay pool channel
            cx.background_spawn({
                let client = client.clone();
                let cache = Arc::clone(&cache);
                let tracker = Arc::clone(&tracker);
                let _ = initialized_at();

                async move {
                    // Connect to the bootstrap relays
                    Self::connect(&client).await;

                    // Handle notifications from the relay pool
                    Self::handle_notifications(&client, &cache, &tracker).await;
                }
            }),
        );

        Self {
            client,
            tracker,
            cache_manager: cache,
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
        cache: &Arc<RwLock<CacheManager>>,
        tracker: &Arc<RwLock<EventTracker>>,
    ) {
        let mut notifications = client.notifications();
        let mut processed_events = HashSet::new();

        while let Ok(notification) = notifications.recv().await {
            let RelayPoolNotification::Message { message, relay_url } = notification else {
                // Skip if the notification is not a message
                continue;
            };

            match message {
                RelayMessage::Event { event, .. } => {
                    if !processed_events.insert(event.id) {
                        // Skip if the event has already been processed
                        continue;
                    }

                    match event.kind {
                        Kind::RelayList => {
                            let mut cache = cache.write().await;
                            cache.insert_relays(&event);

                            drop(cache);

                            // Fetch user's messaging relays event
                            _ = Self::subscribe(client, &event, Kind::InboxRelays).await;

                            // Fetch user's encryption announcement event
                            _ = Self::subscribe(client, &event, Kind::Custom(10044)).await;

                            // Fetch user's metadata event
                            _ = Self::subscribe(client, &event, Kind::Metadata).await;

                            // Fetch user's contact list event
                            _ = Self::subscribe(client, &event, Kind::ContactList).await;
                        }
                        Kind::InboxRelays => {
                            let mut cache = cache.write().await;
                            cache.insert_messaging_relays(&event);

                            drop(cache);

                            // Fetch user's inbox messages
                            _ = Self::get_messages(client, &event).await;
                        }
                        Kind::Custom(10044) => {
                            let mut cache = cache.write().await;
                            cache.insert_announcement(&event);

                            drop(cache);
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
                    let mut tracker = tracker.write().await;

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
    pub async fn is_self_authored(client: &Client, event: &Event) -> bool {
        if let Ok(signer) = client.signer().await {
            if let Ok(public_key) = signer.get_public_key().await {
                return public_key == event.pubkey;
            }
        }
        false
    }

    /// Subscribe for events that match the given kind for a given author
    async fn subscribe(client: &Client, relay: &Event, kind: Kind) -> Result<(), Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        if relay.pubkey != public_key {
            return Err(anyhow!("Messaging Relays does not belong to the user"));
        };

        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let filter = Filter::new().author(public_key).kind(kind).limit(1);

        // Extract relays from the relay event
        let urls: Vec<RelayUrl> = nip65::extract_relay_list(relay)
            .filter_map(|(url, metadata)| {
                if metadata.is_none() || metadata == &Some(RelayMetadata::Write) {
                    Some(url)
                } else {
                    None
                }
            })
            .cloned()
            .collect();

        // Verify that there are relays provided
        if urls.is_empty() {
            return Err(anyhow!("No relays provided"));
        }

        // Ensure relay connection
        for url in urls.iter() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Subscribe to filters from the user's write relays
        client.subscribe_to(urls, filter, Some(opts)).await?;

        Ok(())
    }

    /// Get all gift wrap events in the messaging relays for a given public key
    async fn get_messages(client: &Client, relay: &Event) -> Result<(), Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        if relay.pubkey != public_key {
            return Err(anyhow!("Messaging Relays does not belong to the user"));
        };

        let id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        // Extract relays from the relay event
        let urls: Vec<RelayUrl> = nip17::extract_relay_list(relay).take(3).cloned().collect();

        // Verify that there are relays provided
        if urls.is_empty() {
            return Err(anyhow!("No relays provided"));
        }

        // Ensure relay connection
        for url in urls.iter() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Subscribe to filters to user's messaging relays
        client.subscribe_with_id_to(urls, id, filter, None).await?;

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

    /// Extract an encryption keys announcement from an event.
    pub fn extract_announcement(event: &Event) -> Result<Announcement, Error> {
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
    pub async fn extract_response(client: &Client, event: &Event) -> Result<Response, Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        if event.pubkey != public_key {
            return Err(anyhow!("Event does not belong to current user"));
        }

        let client_pubkey = event
            .tags
            .find(TagKind::custom("P"))
            .and_then(|tag| tag.content())
            .and_then(|c| PublicKey::parse(c).ok())
            .context("Cannot parse public key from the event's tags")?;

        Ok(Response::new(event.content.clone(), client_pubkey))
    }

    /// Returns a reference to the nostr client.
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Returns a reference to the event tracker.
    pub fn tracker(&self) -> Arc<RwLock<EventTracker>> {
        Arc::clone(&self.tracker)
    }

    /// Returns a reference to the cache manager.
    pub fn cache_manager(&self) -> Arc<RwLock<CacheManager>> {
        Arc::clone(&self.cache_manager)
    }
}
