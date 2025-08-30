use std::sync::OnceLock;
use std::time::Duration;

use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use paths::nostr_file;
use smol::channel::{Receiver, Sender};
use smol::lock::RwLock;

use crate::paths::support_dir;

pub mod constants;
pub mod paths;

#[derive(Debug, Clone)]
pub struct AuthReq {
    pub challenge: String,
    pub url: RelayUrl,
}

impl AuthReq {
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

/// Signals sent through the global event channel to notify UI
#[derive(Debug)]
pub enum IngesterSignal {
    /// A signal to notify UI that the client's signer has been set
    SignerSet(PublicKey),

    /// A signal to notify UI that the client's signer has been unset
    SignerUnset,

    /// A signal to notify UI that the relay requires authentication
    Auth(AuthReq),

    /// A signal to notify UI that the browser proxy service is down
    ProxyDown,

    /// A signal to notify UI that a new metadata event has been received
    Metadata(Event),

    /// A signal to notify UI that a new gift wrap event has been received
    GiftWrap((EventId, Event)),

    /// A signal to notify UI that all gift wrap events have been processed
    Finish,

    /// A signal to notify UI that partial processing of gift wrap events has been completed
    PartialFinish,

    /// A signal to notify UI that no DM relay for current user was found
    DmRelayNotFound,

    /// A signal to notify UI that there are errors or notices occurred
    Notice(Notice),
}

#[derive(Debug)]
pub struct Ingester {
    rx: Receiver<IngesterSignal>,
    tx: Sender<IngesterSignal>,
}

impl Default for Ingester {
    fn default() -> Self {
        Self::new()
    }
}

impl Ingester {
    pub fn new() -> Self {
        let (tx, rx) = smol::channel::bounded::<IngesterSignal>(2048);
        Self { rx, tx }
    }

    pub fn signals(&self) -> &Receiver<IngesterSignal> {
        &self.rx
    }

    pub async fn send(&self, signal: IngesterSignal) {
        if let Err(e) = self.tx.send(signal).await {
            log::error!("Failed to send signal: {e}");
        }
    }
}

static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();

static INGESTER: OnceLock<Ingester> = OnceLock::new();

static SENT_IDS: OnceLock<RwLock<Vec<EventId>>> = OnceLock::new();

static CURRENT_TIMESTAMP: OnceLock<Timestamp> = OnceLock::new();

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
            // Sleep after idle for 30 seconds
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(30),
            });

        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}

pub fn ingester() -> &'static Ingester {
    INGESTER.get_or_init(Ingester::new)
}

pub fn starting_time() -> &'static Timestamp {
    CURRENT_TIMESTAMP.get_or_init(Timestamp::now)
}

pub fn sent_ids() -> &'static RwLock<Vec<EventId>> {
    SENT_IDS.get_or_init(|| RwLock::new(Vec::new()))
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
