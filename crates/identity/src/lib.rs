use gpui::{App, AppContext, Context, Entity, Global, Window};
use nostr_connect::prelude::*;

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) {
    Identity::set_global(cx.new(|cx| Identity::new(public_key, window, cx)), cx);
}

struct GlobalIdentity(Entity<Identity>);

impl Global for GlobalIdentity {}

pub struct Identity {
    public_key: PublicKey,
    nip17_relays: Option<bool>,
    nip65_relays: Option<bool>,
    temp_keys: Option<Keys>,
}

impl Identity {
    /// Retrieve the Global Identity instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalIdentity>().0.clone()
    }

    /// Retrieve the Identity instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalIdentity>().0.read(cx)
    }

    /// Check if the Global Identity instance has been set
    pub fn has_global(cx: &App) -> bool {
        cx.has_global::<GlobalIdentity>()
    }

    /// Set the Global Identity instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalIdentity(state));
    }

    pub(crate) fn new(
        public_key: PublicKey,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            public_key,
            nip17_relays: None,
            nip65_relays: None,
            temp_keys: None,
        }
    }

    /// Returns the current identity's public key
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    /// Returns the current identity's temporary keys
    pub fn temp_keys(&self) -> Option<&Keys> {
        self.temp_keys.as_ref()
    }

    pub fn set_temp_keys(&mut self, keys: Option<Keys>, cx: &mut Context<Self>) {
        self.temp_keys = keys;
        cx.notify();
    }

    /// Returns the current identity's NIP-17 relays status
    pub fn nip17_relays(&self) -> Option<bool> {
        self.nip17_relays
    }

    /// Returns the current identity's NIP-65 relays status
    pub fn nip65_relays(&self) -> Option<bool> {
        self.nip65_relays
    }
}
