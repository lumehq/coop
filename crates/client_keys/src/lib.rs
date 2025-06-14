use std::{cell::RefCell, rc::Rc};

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
    keys: Rc<RefCell<Option<Keys>>>,
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
        let mut subscriptions = smallvec![];

        subscriptions.push(cx.observe_new::<Self>(|this, window, cx| {
            if let Some(window) = window {
                this.load(window, cx);
            }
        }));

        Self {
            keys: Rc::new(RefCell::new(None)),
            subscriptions,
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read_client_keys = cx.read_credentials(KEYRING_CLIENT_PATH);

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((_, secret))) = read_client_keys.await else {
                this.update(cx, |this, cx| {
                    // The UI still needs to be notified in this case
                    *this.keys.borrow_mut() = None;
                    cx.notify();
                })
                .ok();
                return;
            };

            this.update(cx, |this, cx| {
                let keys = if shared_state().first_run {
                    let keys = Keys::generate();
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

                    keys
                } else {
                    let Ok(secret_key) = SecretKey::from_slice(&secret) else {
                        // The UI still needs to be notified in this case
                        *this.keys.borrow_mut() = None;
                        cx.notify();

                        return;
                    };

                    Keys::new(secret_key)
                };

                *this.keys.borrow_mut() = Some(keys);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn new_keys(&mut self, cx: &mut Context<Self>) {
        let keys = Keys::generate();
        *self.keys.borrow_mut() = Some(keys);
        cx.notify();
    }

    pub fn keys(&self) -> Keys {
        self.keys
            .borrow()
            .clone()
            .expect("Keys should always be initialized")
    }

    pub fn has_keys(&self) -> bool {
        self.keys.borrow().is_some()
    }
}
