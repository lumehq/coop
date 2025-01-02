use gpui::*;
use nostr_sdk::prelude::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

pub struct MetadataRegistry {
    seens: Arc<Mutex<Vec<PublicKey>>>,
    profiles: Arc<RwLock<HashMap<PublicKey, Metadata>>>,
}

impl Global for MetadataRegistry {}

impl MetadataRegistry {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());
    }

    pub fn seen(&mut self, public_key: PublicKey, metadata: Option<Metadata>) {
        let mut seens = self.seens.lock().unwrap();

        if !seens.contains(&public_key) {
            seens.push(public_key);
            drop(seens);

            if let Some(metadata) = metadata {
                self.profiles.write().unwrap().insert(public_key, metadata);
            }
        }
    }

    pub fn get(&self, public_key: &PublicKey) -> Option<Metadata> {
        self.profiles.read().unwrap().get(public_key).cloned()
    }

    fn new() -> Self {
        let seens = Arc::new(Mutex::new(Vec::new()));
        let profiles = Arc::new(RwLock::new(HashMap::new()));

        Self { seens, profiles }
    }
}
