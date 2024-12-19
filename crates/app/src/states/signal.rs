use gpui::*;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone)]
pub enum Signal {
    /// Request metadata
    ReqMetadata(PublicKey),
    /// Receive metadata
    RecvMetadata(PublicKey),
    /// Receive EOSE
    RecvEose(SubscriptionId),
    /// Receive event
    RecvEvent(Event),
}

pub struct SignalRegistry {
    pub tx: Arc<UnboundedSender<PublicKey>>,
}

impl Global for SignalRegistry {}

impl SignalRegistry {
    pub fn set_global(cx: &mut AppContext, tx: UnboundedSender<PublicKey>) {
        cx.set_global(Self::new(tx));
    }

    fn new(tx: UnboundedSender<PublicKey>) -> Self {
        Self { tx: Arc::new(tx) }
    }
}
