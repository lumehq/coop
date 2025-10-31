use flume::{Receiver, Sender};
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NewMessage {
    pub gift_wrap: EventId,
    pub rumor: UnsignedEvent,
}

impl NewMessage {
    pub fn new(gift_wrap: EventId, rumor: UnsignedEvent) -> Self {
        Self { gift_wrap, rumor }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Announcement {
    id: EventId,
    client: String,
    public_key: PublicKey,
}

impl Announcement {
    pub fn new(id: EventId, client_name: String, public_key: PublicKey) -> Self {
        Self {
            id,
            client: client_name,
            public_key,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn client(&self) -> &str {
        self.client.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Response {
    payload: String,
    public_key: PublicKey,
}

impl Response {
    pub fn new(payload: String, public_key: PublicKey) -> Self {
        Self {
            payload,
            public_key,
        }
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn payload(&self) -> &str {
        self.payload.as_str()
    }
}

/// Signals sent through the global event channel to notify UI
#[derive(Debug)]
pub enum SignalKind {
    /// NIP-4e
    ///
    /// A signal to notify UI that the user has not set encryption keys yet
    EncryptionNotSet,

    /// NIP-4e
    ///
    /// A signal to notify UI that the user has set encryption keys
    EncryptionSet(Announcement),

    /// NIP-4e
    ///
    /// A signal to notify UI that the user has responded to an encryption request
    EncryptionResponse(Response),

    /// NIP-4e
    ///
    /// A signal to notify UI that the user has requested encryption keys from other devices
    EncryptionRequest(Announcement),

    /// A signal to notify UI that the client's signer has been set
    SignerSet(PublicKey),

    /// A signal to notify UI that the relay requires authentication
    Auth(AuthRequest),

    /// A signal to notify UI that a new profile has been received
    NewProfile(Profile),

    /// A signal to notify UI that a new gift wrap event has been received
    NewMessage(NewMessage),

    /// A signal to notify UI that no messaging relays for current user was found
    MessagingRelaysNotFound,

    /// A signal to notify UI that no gossip relays for current user was found
    GossipRelaysNotFound,

    /// A signal to notify UI that gift wrap status has changed
    GiftWrapStatus(UnwrappingStatus),
}

#[derive(Debug, Clone)]
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

    pub fn sender(&self) -> &Sender<SignalKind> {
        &self.tx
    }

    pub async fn send(&self, kind: SignalKind) {
        if let Err(e) = self.tx.send_async(kind).await {
            log::error!("Failed to send signal: {e}");
        }
    }
}
