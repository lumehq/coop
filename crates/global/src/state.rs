use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, Error};
use flume::{Receiver, Sender};
use nostr_sdk::prelude::*;
use smol::lock::RwLock;

use crate::nostr_client;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppIdentifierTag {
    Master,
    Client,
    Bunker,
    User,
    Setting,
}

impl Display for AppIdentifierTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppIdentifierTag::Master => write!(f, "coop:master"),
            AppIdentifierTag::Client => write!(f, "coop:client"),
            AppIdentifierTag::Bunker => write!(f, "coop:bunker"),
            AppIdentifierTag::User => write!(f, "coop:user"),
            AppIdentifierTag::Setting => write!(f, "coop:setting"),
        }
    }
}

impl From<AppIdentifierTag> for String {
    fn from(val: AppIdentifierTag) -> Self {
        val.to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct NostrDevice {
    /// Local keys used for communication between devices
    client: Option<Arc<dyn NostrSigner>>,
    /// A signer used for decrypting messages.
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    master: Option<Arc<dyn NostrSigner>>,
}

impl NostrDevice {
    pub fn master(&self) -> Option<&Arc<dyn NostrSigner>> {
        self.master.as_ref()
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
            sent_ids: RwLock::new(HashSet::new()),
            seen_on_relays: RwLock::new(HashMap::new()),
            resent_ids: RwLock::new(Vec::new()),
            resend_queue: RwLock::new(HashMap::new()),
        }
    }

    pub async fn write_to_db(&self, content: &str, id: AppIdentifierTag) -> Result<(), Error> {
        let client = nostr_client();
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        // Construct a application data event (NIP-78)
        //
        // Only sign with a random keys
        let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
            .tag(Tag::identifier(id))
            .build(public_key)
            .sign(&Keys::generate())
            .await?;

        // Store the event to the local database
        client.database().save_event(&event).await?;

        Ok(())
    }

    pub async fn load_from_db(&self, id: AppIdentifierTag) -> Result<String, Error> {
        let client = nostr_client();
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .author(public_key)
            .identifier(id)
            .limit(1);

        if let Some(event) = client.database().query(filter).await?.first_owned() {
            Ok(event.content)
        } else {
            Err(anyhow!("Not found"))
        }
    }
}
