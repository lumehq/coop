use global::{constants::KEYRING_CLIENT_PATH, shared_state};
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Window};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};

pub fn init(cx: &mut App) {
    ClientKeys::set_global(cx.new(ClientKeys::new), cx);
}

struct GlobalClientKeys(Entity<ClientKeys>);

impl Global for GlobalClientKeys {}

pub struct ClientKeys {
    keys: Entity<Option<Keys>>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl ClientKeys {
    /// Retrieve the Global Settings instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalClientKeys>().0.clone()
    }

    /// Retrieve the Settings instance
    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalClientKeys>().0.read(cx)
    }

    /// Set the global Settings instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalClientKeys(state));
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let keys = cx.new(|_| None);
        let mut subscriptions = smallvec![];

        subscriptions.push(cx.observe_new::<Self>(|this, window, cx| {
            if let Some(window) = window {
                this.load(window, cx);
            }
        }));

        Self {
            keys,
            subscriptions,
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read_client_keys = cx.read_credentials(KEYRING_CLIENT_PATH);

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Some((_, secret))) = read_client_keys.await {
                // Update keys
                this.update(cx, |this, cx| {
                    let Ok(secret_key) = SecretKey::from_slice(&secret) else {
                        this.set_keys(None, false, cx);
                        return;
                    };
                    let keys = Keys::new(secret_key);
                    this.set_keys(Some(keys), false, cx);
                })
                .ok();
            } else if shared_state().first_run {
                // Generate a new keys and update
                this.update(cx, |this, cx| {
                    this.new_keys(cx);
                })
                .ok();
            } else {
                this.update(cx, |this, cx| {
                    this.set_keys(None, false, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn set_keys(&mut self, keys: Option<Keys>, persist: bool, cx: &mut Context<Self>) {
        if let Some(keys) = keys.clone() {
            if persist {
                let write_keys = cx.write_credentials(
                    KEYRING_CLIENT_PATH,
                    keys.public_key().to_hex().as_str(),
                    keys.secret_key().as_secret_bytes(),
                );

                cx.background_spawn(async move {
                    if let Err(e) = write_keys.await {
                        log::error!("Failed to save the client keys: {e}")
                    }
                })
                .detach();

                cx.notify();
            }
        }

        self.keys.update(cx, |this, cx| {
            *this = keys;
            cx.notify();
        });
    }

    pub fn new_keys(&mut self, cx: &mut Context<Self>) {
        self.set_keys(Some(Keys::generate()), true, cx);
    }

    pub fn keys(&self, cx: &App) -> Keys {
        self.keys
            .read(cx)
            .as_ref()
            .cloned()
            .expect("Keys should always be initialized")
    }

    pub fn has_keys(&self, cx: &App) -> bool {
        self.keys.read(cx).is_some()
    }
}
