use gpui::*;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub enum Signal {
    /// Send
    DONE(PublicKey),
    /// Receive
    REQ(PublicKey),
}

pub struct MetadataRegistry {
    seens: Vec<PublicKey>,
    pub reqs: Sender<Signal>,
}

impl Global for MetadataRegistry {}

impl MetadataRegistry {
    pub fn set_global(cx: &mut AppContext, reqs: Sender<Signal>) {
        cx.set_global(Self::new(reqs));
    }

    pub fn contains(&self, public_key: PublicKey) -> bool {
        self.seens.contains(&public_key)
    }

    pub fn seen(&mut self, public_key: PublicKey) {
        if !self.seens.contains(&public_key) {
            self.seens.push(public_key);
        }
    }

    fn new(reqs: Sender<Signal>) -> Self {
        Self {
            seens: Vec::new(),
            reqs,
        }
    }
}
