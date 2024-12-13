use gpui::*;
use nostr_sdk::prelude::*;

pub struct SignalRegistry {
    public_keys: Vec<PublicKey>,
}

impl Global for SignalRegistry {}

impl SignalRegistry {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());
    }

    pub fn contains(&self, public_key: PublicKey) -> bool {
        self.public_keys.contains(&public_key)
    }

    pub fn push(&mut self, public_key: PublicKey) {
        self.public_keys.push(public_key);
    }

    fn new() -> Self {
        Self {
            public_keys: Vec::new(),
        }
    }
}
