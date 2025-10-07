use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use nostr_lmdb::NostrLMDB;
use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::lock::RwLock;

use crate::ingester::Ingester;
use crate::signal::Signal;

pub mod constants;
pub mod ingester;
pub mod paths;
pub mod signal;

#[derive(Debug, Default)]
pub struct Gossip {
    pub nip65: HashMap<PublicKey, HashSet<(RelayUrl, Option<RelayMetadata>)>>,
    pub nip17: HashMap<PublicKey, HashSet<RelayUrl>>,
}

#[derive(Debug, Default)]
pub struct NostrDevice {
    /// A signer used for encryption.
    encryption: Option<Arc<dyn NostrSigner>>,
    /// A signer used for communication.
    client: Option<Arc<dyn NostrSigner>>,
}

impl NostrDevice {
    pub fn encryption(&self) -> Option<&Arc<dyn NostrSigner>> {
        self.encryption.as_ref()
    }

    pub fn client(&self) -> Option<&Arc<dyn NostrSigner>> {
        self.client.as_ref()
    }
}

/// A simple storage to store all states that using across the application.
#[derive(Debug)]
pub struct AppState {
    /// The timestamp when the application was initialized.
    pub init_at: Timestamp,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub device: RwLock<NostrDevice>,

    /// Subscription ID for listening to gift wrap events from relays.
    pub gift_wrap_sub_id: SubscriptionId,

    /// Auto-close options for relay subscriptions
    pub auto_close_opts: Option<SubscribeAutoCloseOptions>,

    /// Whether gift wrap processing is in progress.
    pub gift_wrap_processing: AtomicBool,

    /// Tracking events sent by Coop in the current session
    pub sent_ids: RwLock<HashSet<EventId>>,

    /// Tracking events seen on which relays in the current session
    pub seen_on_relays: RwLock<HashMap<EventId, HashSet<RelayUrl>>>,

    /// Tracking events that have been resent by Coop in the current session
    pub resent_ids: RwLock<Vec<Output<EventId>>>,

    /// Temporarily store events that need to be resent later
    pub resend_queue: RwLock<HashMap<EventId, RelayUrl>>,

    /// A storage for Coop's self-implemented gossip
    pub gossip: RwLock<Gossip>,

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
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let signal = Signal::default();
        let ingester = Ingester::default();

        Self {
            init_at,
            signal,
            ingester,
            gift_wrap_sub_id: SubscriptionId::new("inbox"),
            gift_wrap_processing: AtomicBool::new(false),
            auto_close_opts: Some(opts),
            device: RwLock::new(NostrDevice::default()),
            gossip: RwLock::new(Gossip::default()),
            sent_ids: RwLock::new(HashSet::new()),
            seen_on_relays: RwLock::new(HashMap::new()),
            resent_ids: RwLock::new(Vec::new()),
            resend_queue: RwLock::new(HashMap::new()),
        }
    }
}

static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn nostr_client() -> &'static Client {
    NOSTR_CLIENT.get_or_init(|| {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let lmdb = NostrLMDB::open(nostr_file()).expect("Database is NOT initialized");

        let opts = ClientOptions::new()
            .gossip(false)
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(600),
            });

        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}

static APP_STATE: OnceLock<AppState> = OnceLock::new();

pub fn app_state() -> &'static AppState {
    APP_STATE.get_or_init(AppState::new)
}
