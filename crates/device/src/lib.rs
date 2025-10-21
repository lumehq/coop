use std::sync::{Arc, LazyLock};

use anyhow::{anyhow, Error};
use gpui::{
    App, AppContext, AsyncWindowContext, Context, Entity, Global, Task, WeakEntity, Window,
};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use states::app_state;
use states::constants::APP_NAME;
use states::paths::config_dir;
use states::state::SignalKind;
use ui::ContextModal;

use crate::keystore::{FileProvider, KeyItem, KeyStore, KeyringProvider};

pub mod keystore;

static DISABLE_KEYRING: LazyLock<bool> =
    LazyLock::new(|| std::env::var("DISABLE_KEYRING").is_ok_and(|value| !value.is_empty()));

pub fn init(window: &mut Window, cx: &mut App) {
    Device::set_global(cx.new(|cx| Device::new(window, cx)), cx);
}

struct GlobalDevice(Entity<Device>);

impl Global for GlobalDevice {}

pub struct Device {
    /// Key Store for storing credentials
    pub keystore: Arc<dyn KeyStore>,

    /// Whether the keystore has been initialized
    pub initialized: bool,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Client keys entity, used for communication between devices
    pub client_keys: Entity<Option<Arc<dyn NostrSigner>>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption keys entity, used for encryption and decryption
    pub encryption_keys: Entity<Option<Arc<dyn NostrSigner>>>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 2]>,
}

impl Device {
    /// Retrieve the global keys state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalDevice>().0.clone()
    }

    /// Set the global keys instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalDevice(state));
    }

    /// Create a new keys instance
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client_keys = cx.new(|_| None);
        let encryption_keys = cx.new(|_| None);

        // Use the file system for keystore in development or when the user specifies it
        let use_file_keystore = cfg!(debug_assertions) || *DISABLE_KEYRING;

        // Construct the keystore
        let keystore: Arc<dyn KeyStore> = if use_file_keystore {
            Arc::new(FileProvider::default())
        } else {
            Arc::new(KeyringProvider)
        };

        // Only used for testing keyring availability on the user's system
        let read_credential = cx.read_credentials("Coop");

        let mut tasks = smallvec![];

        tasks.push(
            // Verify the keyring availability
            cx.spawn(async move |this, cx| {
                let result = read_credential.await;

                this.update(cx, |this, cx| {
                    if let Err(e) = result {
                        log::error!("Keyring error: {e}");
                        // For Linux:
                        // The user has not installed secret service on their system
                        // Fall back to the file provider
                        this.keystore = Arc::new(FileProvider::default());
                    }
                    this.initialized = true;
                    cx.notify();
                })
                .ok();
            }),
        );

        tasks.push(
            // Continuously handle signals from the application state
            cx.spawn_in(window, async move |this, cx| {
                Self::handle_signals(this, cx).await
            }),
        );

        Self {
            client_keys,
            encryption_keys,
            keystore,
            initialized: false,
            _tasks: tasks,
        }
    }

    /// Handle signals from the application state
    async fn handle_signals(view: WeakEntity<Device>, cx: &mut AsyncWindowContext) {
        let states = app_state();

        while let Ok(signal) = states.signal().receiver().recv_async().await {
            view.update_in(cx, |this, window, cx| {
                match signal {
                    SignalKind::EncryptionNotSet => {
                        this.new_encryption(window, cx);
                    }
                    SignalKind::EncryptionSet((n, client_name)) => {
                        this.reinit_encryption(n, client_name, window, cx);
                    }
                    _ => {}
                };
            })
            .ok();
        }
    }

    /// Returns the keystore.
    pub fn keystore(&self) -> Arc<dyn KeyStore> {
        Arc::clone(&self.keystore)
    }

    /// Returns true if the keystore is a file keystore.
    pub fn is_using_file_keystore(&self) -> bool {
        self.keystore.name() == "file"
    }

    /// Load the dedicated keys for the current device (client)
    pub fn load_client_keys(&mut self, cx: &mut Context<Self>) {
        let keystore = self.keystore();
        let url = KeyItem::Client;

        cx.spawn(async move |this, cx| {
            let result = keystore.read_credentials(&url.to_string(), cx).await;

            this.update(cx, |this, cx| {
                match result {
                    Ok(Some((_, secret))) => {
                        let secret = SecretKey::from_slice(&secret).unwrap();
                        let keys = Keys::new(secret);

                        this.set_client_keys(keys, cx);
                    }
                    Ok(None) => {
                        if first_run() {
                            this.set_client_keys(Keys::generate(), cx);
                        } else {
                            this.client_keys.update(cx, |this, cx| {
                                *this = None;
                                cx.notify();
                            });
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to load client keys: {e}");

                        this.client_keys.update(cx, |this, cx| {
                            *this = None;
                            cx.notify();
                        });
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    /// Set the client keys
    pub fn set_client_keys(&mut self, keys: Keys, cx: &mut Context<Self>) {
        let keystore = self.keystore();
        let url = KeyItem::Client;
        let username = keys.public_key().to_hex();
        let password = keys.secret_key().to_secret_bytes();

        // Update the client keys
        self.client_keys.update(cx, |this, cx| {
            *this = Some(Arc::new(keys));
            cx.notify();
        });

        // Write the client keys to the keystore
        cx.spawn(async move |_this, cx| {
            if let Err(e) = keystore
                .write_credentials(&url.to_string(), &username, &password, cx)
                .await
            {
                log::error!("Keystore error: {e}")
            }
        })
        .detach();
    }

    /// Set the encryption keys
    pub fn set_encryption_keys(&mut self, keys: Keys, window: &mut Window, cx: &mut Context<Self>) {
        let keystore = self.keystore();
        let url = KeyItem::Encryption;
        let username = keys.public_key().to_hex();
        let password = keys.secret_key().to_secret_bytes();

        // Update the client keys
        self.encryption_keys.update(cx, |this, cx| {
            *this = Some(Arc::new(keys));
            cx.notify();
        });

        // Write the client keys to the keystore
        cx.spawn_in(window, async move |this, cx| {
            if let Err(e) = keystore
                .write_credentials(&url.to_string(), &username, &password, cx)
                .await
            {
                log::error!("Keystore error: {e}")
            } else {
                this.update_in(cx, |_, window, cx| {
                    window.push_notification("Encryption keys have been set successfully", cx);
                })
                .ok();
            };
        })
        .detach();
    }

    fn new_encryption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = Keys::generate();
        let username = keys.public_key().to_hex();
        let password = keys.secret_key().to_secret_bytes();

        let device = Device::global(cx);
        let keystore = device.read(cx).keystore();
        let url = KeyItem::Encryption;

        cx.spawn_in(window, async move |this, cx| {
            let result = keystore
                .write_credentials(&url.to_string(), &username, &password, cx)
                .await;

            this.update_in(cx, |_this, window, cx| {
                match result {
                    Ok(_) => {
                        device.update(cx, |this, cx| {
                            this.set_encryption_keys(keys, window, cx);
                        });
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn reinit_encryption(
        &mut self,
        n: PublicKey,
        client_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let device = Device::global(cx);
        let keystore = device.read(cx).keystore();
        let url = KeyItem::Encryption;

        cx.spawn_in(window, async move |this, cx| {
            let result = keystore.read_credentials(&url.to_string(), cx).await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Some((username, password))) => {
                        let public_key = PublicKey::from_hex(&username).unwrap();

                        if n == public_key {
                            let secret = SecretKey::from_slice(&password).unwrap();
                            let keys = Keys::new(secret);

                            device.update(cx, |this, cx| {
                                this.set_encryption_keys(keys, window, cx);
                            });
                        } else {
                            this.request_encryption(client_name, window, cx);
                        }
                    }
                    Ok(None) => {
                        this.render_request_encryption(client_name, window, cx);
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn request_encryption(
        &mut self,
        _client_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let device = Device::global(cx);

        // Client Keys must be known at this point
        let Some(client_keys) = device.read(cx).client_keys.read(cx).clone() else {
            window.push_notification("Client Keys is required", cx);
            return;
        };

        let task: Task<Result<Option<Keys>, Error>> = cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let client_pubkey = client_keys.get_public_key().await?;

            let filter = Filter::new()
                .author(public_key)
                .kind(Kind::Custom(4455))
                .limit(1);

            match client.database().query(filter).await?.first_owned() {
                Some(event) => {
                    // Found encryption keys shared by other devices
                    if let Some(target) = event
                        .tags
                        .find(TagKind::custom("P"))
                        .and_then(|tag| tag.content())
                        .and_then(|content| PublicKey::parse(content).ok())
                        .as_ref()
                    {
                        let payload = event.content.as_str();
                        let decrypted = client_keys.nip44_decrypt(target, payload).await?;

                        let secret = SecretKey::from_hex(&decrypted)?;
                        let keys = Keys::new(secret);

                        return Ok(Some(keys));
                    } else {
                        return Err(anyhow!("Invalid event"));
                    }
                }
                None => {
                    // Construct encryption keys request event
                    let event = EventBuilder::new(Kind::Custom(4454), "")
                        .tags(vec![Tag::client(APP_NAME), Tag::public_key(client_pubkey)])
                        .sign(&signer)
                        .await?;

                    // Send a request for encryption keys from other devices
                    client.send_event(&event).await?;
                }
            }

            Ok(None)
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Some(keys)) => {
                        device.update(cx, |this, cx| {
                            this.set_encryption_keys(keys, window, cx);
                        });
                    }
                    Ok(None) => {
                        this.render_wait_for_approval(window, cx);
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn render_request_encryption(
        &mut self,
        _client_name: String,
        _window: &mut Window,
        _cx: &mut App,
    ) {
        //
    }

    fn render_wait_for_approval(&mut self, _window: &mut Window, _cx: &mut App) {
        //
    }
}

fn first_run() -> bool {
    let flag = config_dir().join(".first_run");
    !flag.exists() && std::fs::write(&flag, "").is_ok()
}
