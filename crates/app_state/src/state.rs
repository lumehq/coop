use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Error};
use flume::{Receiver, Sender};
use nostr_sdk::prelude::*;
use smol::lock::RwLock;

use crate::constants::BOOTSTRAP_RELAYS;
use crate::nostr_client;
use crate::paths::support_dir;

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

    /// A signal to notify UI that no DM relays for current user was found
    RelaysNotFound,

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
pub struct Gossip {
    pub nip17: HashMap<PublicKey, HashSet<RelayUrl>>,
    pub nip65: HashMap<PublicKey, HashSet<(RelayUrl, Option<RelayMetadata>)>>,
}

impl Gossip {
    pub fn insert(&mut self, event: &Event) {
        match event.kind {
            Kind::InboxRelays => {
                let urls: Vec<RelayUrl> = nip17::extract_relay_list(event).cloned().collect();

                if !urls.is_empty() {
                    self.nip17.entry(event.pubkey).or_default().extend(urls);
                }
            }
            Kind::RelayList => {
                let urls: Vec<(RelayUrl, Option<RelayMetadata>)> = nip65::extract_relay_list(event)
                    .map(|(url, metadata)| (url.to_owned(), metadata.to_owned()))
                    .collect();

                if !urls.is_empty() {
                    self.nip65.entry(event.pubkey).or_default().extend(urls);
                }
            }
            _ => {}
        }
    }

    pub fn write_relays(&self, public_key: &PublicKey) -> Vec<&RelayUrl> {
        self.nip65
            .get(public_key)
            .map(|relays| {
                relays
                    .iter()
                    .filter(|(_, metadata)| metadata.as_ref() != Some(&RelayMetadata::Write))
                    .map(|(url, _)| url)
                    .take(3)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn read_relays(&self, public_key: &PublicKey) -> Vec<&RelayUrl> {
        self.nip65
            .get(public_key)
            .map(|relays| {
                relays
                    .iter()
                    .filter(|(_, metadata)| metadata.as_ref() != Some(&RelayMetadata::Read))
                    .map(|(url, _)| url)
                    .take(3)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn messaging_relays(&self, public_key: &PublicKey) -> Vec<&RelayUrl> {
        self.nip17
            .get(public_key)
            .map(|relays| relays.iter().collect())
            .unwrap_or_default()
    }

    pub async fn subscribe(&self, public_key: PublicKey, kind: Kind) -> Result<(), Error> {
        let client = nostr_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new().author(public_key).kind(kind).limit(1);
        let urls = self.write_relays(&public_key);

        // Ensure user's have at least one write relay
        if urls.is_empty() {
            return Err(anyhow!("NIP-65 relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Subscribe to filters to user's write relays
        client.subscribe_to(urls, filter, Some(opts)).await?;

        Ok(())
    }

    pub async fn metadata_subscribes(&self, public_keys: HashSet<PublicKey>) -> Result<(), Error> {
        if public_keys.is_empty() {
            return Err(anyhow!("You need at least one public key"));
        }

        let client = nostr_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];
        let limit = public_keys.len() * kinds.len() + 20;

        let filter = Filter::new().authors(public_keys).kinds(kinds).limit(limit);
        let urls = BOOTSTRAP_RELAYS;

        // Subscribe to filters to the bootstrap relays
        client.subscribe_to(urls, filter, Some(opts)).await?;

        Ok(())
    }

    pub async fn subscribe_to_inbox(&self, public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let id = SubscriptionId::new("inbox");
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
        let urls = self.messaging_relays(&public_key);

        // Ensure user's have at least one messaging relay
        if urls.is_empty() {
            return Err(anyhow!("Messaging relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Subscribe to filters to user's messaging relays
        client.subscribe_with_id_to(urls, id, filter, None).await?;

        Ok(())
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
    pub init_at: Timestamp,

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
        let init_at = Timestamp::now();
        let first_run = first_run();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let signal = Signal::default();
        let ingester = Ingester::default();

        Self {
            init_at,
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
}

fn first_run() -> bool {
    let flag = support_dir().join(format!(".{}-first_run", env!("CARGO_PKG_VERSION")));

    if !flag.exists() {
        if std::fs::write(&flag, "").is_err() {
            return false;
        }
        true // First run
    } else {
        false // Not first run
    }
}
