use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use nostr_sdk::prelude::*;
use smol::lock::RwLock;

static TRACKER: OnceLock<Arc<RwLock<EventTracker>>> = OnceLock::new();

pub fn tracker() -> &'static Arc<RwLock<EventTracker>> {
    TRACKER.get_or_init(|| Arc::new(RwLock::new(EventTracker::default())))
}

/// Event tracker
#[derive(Debug, Clone, Default)]
pub struct EventTracker {
    /// Tracking events sent by Coop in the current session
    sent_ids: HashSet<EventId>,

    /// Events that need to be resent later
    pending_resend: HashSet<(EventId, RelayUrl)>,
}

impl EventTracker {
    /// Check if an event was sent by Coop in the current session.
    pub fn is_sent_by_coop(&self, id: &EventId) -> bool {
        self.sent_ids.contains(id)
    }

    /// Mark an event as sent by Coop.
    pub fn sent(&mut self, id: EventId) {
        self.sent_ids.insert(id);
    }

    /// Get all events that need to be resent later for a specific relay.
    pub fn pending_resend(&mut self, relay: &RelayUrl) -> Vec<EventId> {
        self.pending_resend
            .extract_if(|(_id, url)| url == relay)
            .map(|(id, _url)| id)
            .collect()
    }

    /// Add an event (id and relay url) to the pending resend set.
    pub fn add_to_pending(&mut self, id: EventId, url: RelayUrl) {
        self.pending_resend.insert((id, url));
    }
}
