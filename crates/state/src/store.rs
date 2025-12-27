use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use nostr_sdk::prelude::*;
use smol::lock::RwLock;

static EVENT_STORE: OnceLock<Arc<RwLock<EventStore>>> = OnceLock::new();

pub fn event_store() -> &'static Arc<RwLock<EventStore>> {
    EVENT_STORE.get_or_init(|| Arc::new(RwLock::new(EventStore::new())))
}

#[derive(Debug, Clone, Default)]
pub struct EventStore {
    /// Timestamp when the EventStore was initialized
    pub initialized_at: Timestamp,

    /// Tracking events that have been resent by Coop in the current session
    pub resent_ids: Vec<Output<EventId>>,

    /// Temporarily store events that need to be resent later
    pub resend_queue: HashMap<EventId, RelayUrl>,

    /// Tracking events sent by Coop in the current session
    pub sent_ids: HashSet<EventId>,

    /// Tracking events seen on which relays in the current session
    pub seen_on_relays: HashMap<EventId, HashSet<RelayUrl>>,
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            initialized_at: Timestamp::now(),
            resent_ids: Vec::new(),
            resend_queue: HashMap::new(),
            sent_ids: HashSet::new(),
            seen_on_relays: HashMap::new(),
        }
    }

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
