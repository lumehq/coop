use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::{config_dir, BOOTSTRAP_RELAYS, SEARCH_RELAYS};
use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_lmdb::NostrLmdb;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use smol::lock::RwLock;
pub use storage::*;
pub use tracker::*;

mod storage;
mod tracker;

pub const GIFTWRAP_SUBSCRIPTION: &str = "gift-wrap-events";

pub fn init(cx: &mut App) {
    NostrRegistry::set_global(cx.new(NostrRegistry::new), cx);
}

struct GlobalNostrRegistry(Entity<NostrRegistry>);

impl Global for GlobalNostrRegistry {}

/// Nostr Registry
#[derive(Debug)]
pub struct NostrRegistry {
    /// Nostr Client
    client: Client,

    /// Custom gossip implementation
    gossip: Arc<RwLock<Gossip>>,

    /// Tracks activity related to Nostr events
    tracker: Arc<RwLock<EventTracker>>,

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

        // Construct the lmdb
        let lmdb = cx.background_executor().block(async move {
            let path = config_dir().join("nostr");
            NostrLmdb::open(path)
                .await
                .expect("Failed to initialize database")
        });

        // Construct the nostr client
        let client = ClientBuilder::default().database(lmdb).opts(opts).build();

        let tracker = Arc::new(RwLock::new(EventTracker::default()));
        let gossip = Arc::new(RwLock::new(Gossip::default()));

        let mut tasks = smallvec![];

        tasks.push(
            // Establish connection to the bootstrap relays
            //
            // And handle notifications from the nostr relay pool channel
            cx.background_spawn({
                let client = client.clone();
                let gossip = Arc::clone(&gossip);
                let tracker = Arc::clone(&tracker);
                let _ = initialized_at();

                async move {
                    // Connect to the bootstrap relays
                    Self::connect(&client).await;

                    // Handle notifications from the relay pool
                    Self::handle_notifications(&client, &gossip, &tracker).await;
                }
            }),
        );

        Self {
            client,
            tracker,
            gossip,
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
        gossip: &Arc<RwLock<Gossip>>,
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
                            let mut gossip = gossip.write().await;
                            gossip.insert_relays(&event);

                            let urls: Vec<RelayUrl> = Self::extract_write_relays(&event);
                            let author = event.pubkey;

                            log::info!("Write relays: {urls:?}");

                            // Fetch user's encryption announcement event
                            Self::get(client, &urls, author, Kind::Custom(10044)).await;
                            // Fetch user's messaging relays event
                            Self::get(client, &urls, author, Kind::InboxRelays).await;

                            // Verify if the event is belonging to the current user
                            if Self::is_self_authored(client, &event).await {
                                // Fetch user's metadata event
                                Self::get(client, &urls, author, Kind::Metadata).await;
                                // Fetch user's contact list event
                                Self::get(client, &urls, author, Kind::ContactList).await;
                            }
                        }
                        Kind::InboxRelays => {
                            let mut gossip = gossip.write().await;
                            gossip.insert_messaging_relays(&event);

                            if Self::is_self_authored(client, &event).await {
                                // Extract user's messaging relays
                                let urls: Vec<RelayUrl> =
                                    nip17::extract_relay_list(&event).cloned().collect();

                                // Fetch user's inbox messages in the extracted relays
                                Self::get_messages(client, event.pubkey, &urls).await;
                            }
                        }
                        Kind::Custom(10044) => {
                            let mut gossip = gossip.write().await;
                            gossip.insert_announcement(&event);
                        }
                        Kind::ContactList => {
                            if Self::is_self_authored(client, &event).await {
                                let public_keys: Vec<PublicKey> =
                                    event.tags.public_keys().copied().collect();

                                if let Err(e) =
                                    Self::get_metadata_for_list(client, public_keys).await
                                {
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

    /// Get event that match the given kind for a given author
    async fn get(client: &Client, urls: &[RelayUrl], author: PublicKey, kind: Kind) {
        // Skip if no relays are provided
        if urls.is_empty() {
            return;
        }

        // Ensure relay connections
        for url in urls.iter() {
            client.add_relay(url).await.ok();
            client.connect_relay(url).await.ok();
        }

        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let filter = Filter::new().author(author).kind(kind).limit(1);

        // Subscribe to filters from the user's write relays
        if let Err(e) = client.subscribe_to(urls, filter, Some(opts)).await {
            log::error!("Failed to subscribe: {}", e);
        }
    }

    /// Get all gift wrap events in the messaging relays for a given public key
    pub async fn get_messages(client: &Client, public_key: PublicKey, urls: &[RelayUrl]) {
        // Verify that there are relays provided
        if urls.is_empty() {
            return;
        }

        // Ensure relay connection
        for url in urls.iter() {
            client.add_relay(url).await.ok();
            client.connect_relay(url).await.ok();
        }

        let id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        // Unsubscribe from the previous subscription
        client.unsubscribe(&id).await;

        // Subscribe to filters to user's messaging relays
        if let Err(e) = client.subscribe_with_id_to(urls, id, filter, None).await {
            log::error!("Failed to subscribe: {}", e);
        } else {
            log::info!("Subscribed to gift wrap events for public key {public_key}",);
        }
    }

    /// Get metadata for a list of public keys
    async fn get_metadata_for_list(client: &Client, pubkeys: Vec<PublicKey>) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList];

        // Return if the list is empty
        if pubkeys.is_empty() {
            return Err(anyhow!("You need at least one public key".to_string(),));
        }

        let filter = Filter::new()
            .limit(pubkeys.len() * kinds.len())
            .authors(pubkeys)
            .kinds(kinds);

        // Subscribe to filters to the bootstrap relays
        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    pub fn extract_read_relays(event: &Event) -> Vec<RelayUrl> {
        nip65::extract_relay_list(event)
            .filter_map(|(url, metadata)| {
                if metadata.is_none() || metadata == &Some(RelayMetadata::Read) {
                    Some(url.to_owned())
                } else {
                    None
                }
            })
            .take(3)
            .collect()
    }

    pub fn extract_write_relays(event: &Event) -> Vec<RelayUrl> {
        nip65::extract_relay_list(event)
            .filter_map(|(url, metadata)| {
                if metadata.is_none() || metadata == &Some(RelayMetadata::Write) {
                    Some(url.to_owned())
                } else {
                    None
                }
            })
            .take(3)
            .collect()
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
            .map(|c| c.to_string());

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
    pub fn gossip(&self) -> Arc<RwLock<Gossip>> {
        Arc::clone(&self.gossip)
    }
}
