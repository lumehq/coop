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

    /// A signal to notify UI that there are errors or notices occurred
    Notice(Notice),
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

/// A simple storage to store all states that using across the application.
#[derive(Debug)]
pub struct CoopSimpleStorage {
    pub init_at: Timestamp,

    pub last_used_at: Option<Timestamp>,

    pub is_first_run: AtomicBool,

    pub gift_wrap_sub_id: SubscriptionId,

    pub gift_wrap_processing: AtomicBool,

    pub auto_close_opts: Option<SubscribeAutoCloseOptions>,

    pub seen_on_relays: RwLock<HashMap<EventId, HashSet<RelayUrl>>>,

    pub sent_ids: RwLock<HashSet<EventId>>,

    pub resent_ids: RwLock<Vec<Output<EventId>>>,

    pub resend_queue: RwLock<HashMap<EventId, RelayUrl>>,

    pub signal: Signal,

    pub ingester: Ingester,
}

impl Default for CoopSimpleStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl CoopSimpleStorage {
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
            last_used_at: None,
            is_first_run: AtomicBool::new(first_run),
            gift_wrap_sub_id: SubscriptionId::new("inbox"),
            gift_wrap_processing: AtomicBool::new(false),
            auto_close_opts: Some(opts),
            seen_on_relays: RwLock::new(HashMap::new()),
            sent_ids: RwLock::new(HashSet::new()),
            resent_ids: RwLock::new(Vec::new()),
            resend_queue: RwLock::new(HashMap::new()),
        }
    }
}

static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();
static COOP_SIMPLE_STORAGE: OnceLock<CoopSimpleStorage> = OnceLock::new();

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

pub fn css() -> &'static CoopSimpleStorage {
    COOP_SIMPLE_STORAGE.get_or_init(CoopSimpleStorage::new)
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
