use gpui::*;
use nostr_sdk::prelude::*;

pub struct MetadataRegistry {
    seens: Vec<PublicKey>,
}

impl Global for MetadataRegistry {}

impl MetadataRegistry {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());
    }

    pub fn contains(&self, public_key: PublicKey) -> bool {
        self.seens.contains(&public_key)
    }

    pub fn seen(&mut self, public_key: PublicKey) {
        if !self.seens.contains(&public_key) {
            self.seens.push(public_key);
        }
    }

    fn new() -> Self {
        Self { seens: Vec::new() }
    }
}
