use gpui::*;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub struct SignalRegistry {
    public_keys: Vec<PublicKey>,
    pub queue: Arc<Sender<PublicKey>>,
}

impl Global for SignalRegistry {}

impl SignalRegistry {
    pub fn set_global(cx: &mut AppContext, queue: Arc<Sender<PublicKey>>) {
        cx.set_global(Self::new(queue));
    }

    pub fn contains(&self, public_key: PublicKey) -> bool {
        self.public_keys.contains(&public_key)
    }

    pub fn push(&mut self, public_key: PublicKey) {
        self.public_keys.push(public_key);
    }

    pub fn add_to_queue(&mut self, public_key: PublicKey) {
        if let Err(e) = self.queue.send(public_key) {
            println!("Dropped: {}", e)
        }
    }

    fn new(queue: Arc<Sender<PublicKey>>) -> Self {
        Self {
            public_keys: Vec::new(),
            queue,
        }
    }
}
