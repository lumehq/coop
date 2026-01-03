use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use nostr_sdk::prelude::*;

static INITIALIZED_AT: OnceLock<Timestamp> = OnceLock::new();

pub fn initialized_at() -> &'static Timestamp {
    INITIALIZED_AT.get_or_init(Timestamp::now)
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
