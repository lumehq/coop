use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use std::time::Duration;

use flume::{Receiver, Sender};
use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::lock::RwLock;

use crate::paths::support_dir;

pub mod constants;
pub mod paths;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthRequest {
    pub challenge: String,
    pub url: RelayUrl,
}

impl AuthRequest {
    pub fn new(challenge: impl Into<String>, url: RelayUrl) -> Self {
        Self {
            challenge: challenge.into(),
            url,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Notice {
    RelayFailed(RelayUrl),
    AuthFailed(RelayUrl),
    Custom(String),
}

impl Notice {
    pub fn as_str(&self) -> String {
        match self {
            Notice::AuthFailed(url) => format!("Authenticate failed for relay {url}"),
            Notice::RelayFailed(url) => format!("Failed to connect the relay {url}"),
            Notice::Custom(msg) => msg.into(),
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
pub enum Signal {
    /// A signal to notify UI that the client's signer has been set
    SignerSet(PublicKey),

    /// A signal to notify UI that the client's signer has been unset
    SignerUnset,

    /// A signal to notify UI that the relay requires authentication
    Auth(AuthRequest),

    /// A signal to notify UI that the browser proxy service is down
    ProxyDown,

    /// A signal to notify UI that a new metadata event has been received
    Metadata(Event),

    /// A signal to notify UI that a new gift wrap event has been received
    Message((EventId, Event)),

    /// A signal to notify UI that gift wrap process status has changed
    GiftWrapProcess(UnwrappingStatus),

    /// A signal to notify UI that no DM relay for current user was found
    DmRelayNotFound,

    /// A signal to notify UI that there are errors or notices occurred
    Notice(Notice),
}

#[derive(Debug)]
pub struct Ingester {
    rx: Receiver<Signal>,
    tx: Sender<Signal>,
}

impl Default for Ingester {
    fn default() -> Self {
        Self::new()
    }
}

impl Ingester {
    pub fn new() -> Self {
        let (tx, rx) = flume::bounded::<Signal>(2048);
        Self { rx, tx }
    }

    pub fn signals(&self) -> &Receiver<Signal> {
        &self.rx
    }

    pub async fn send(&self, signal: Signal) {
        if let Err(e) = self.tx.send_async(signal).await {
            log::error!("Failed to send signal: {e}");
        }
    }
}

/// A simple storage to store all runtime states that using across the application.
#[derive(Debug)]
pub struct CoopSimpleStorage {
    pub init_at: Timestamp,
    pub gift_wrap_sub_id: SubscriptionId,
    pub gift_wrap_processing: AtomicBool,
    pub sent_ids: RwLock<HashSet<EventId>>,
    pub resent_ids: RwLock<Vec<Output<EventId>>>,
    pub resend_queue: RwLock<HashMap<EventId, RelayUrl>>,
}

impl Default for CoopSimpleStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl CoopSimpleStorage {
    pub fn new() -> Self {
        Self {
            init_at: Timestamp::now(),
            gift_wrap_sub_id: SubscriptionId::new("inbox"),
            gift_wrap_processing: AtomicBool::new(false),
            sent_ids: RwLock::new(HashSet::new()),
            resent_ids: RwLock::new(Vec::new()),
            resend_queue: RwLock::new(HashMap::new()),
        }
    }
}

static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();
static INGESTER: OnceLock<Ingester> = OnceLock::new();
static COOP_SIMPLE_STORAGE: OnceLock<CoopSimpleStorage> = OnceLock::new();
static FIRST_RUN: OnceLock<bool> = OnceLock::new();

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
            .gossip(true)
            .automatic_authentication(false)
            .verify_subscriptions(false)
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(30),
            });

        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}

pub fn ingester() -> &'static Ingester {
    INGESTER.get_or_init(Ingester::new)
}

pub fn css() -> &'static CoopSimpleStorage {
    COOP_SIMPLE_STORAGE.get_or_init(CoopSimpleStorage::new)
}

pub fn first_run() -> &'static bool {
    FIRST_RUN.get_or_init(|| {
        let flag = support_dir().join(format!(".{}-first_run", env!("CARGO_PKG_VERSION")));

        if !flag.exists() {
            if std::fs::write(&flag, "").is_err() {
                return false;
            }
            true // First run
        } else {
            false // Not first run
        }
    })
}
