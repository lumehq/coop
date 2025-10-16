use std::sync::Arc;

use anyhow::{anyhow, Error};
use app_state::constants::KEYRING_URL;
use app_state::nostr_client;
use base64::Engine as _;
use gpui::{App, AppContext, Context, Entity, Global, Task, Window};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};

use crate::item::KeyItem;

pub mod item;

pub fn init(cx: &mut App) {
    KeyStore::set_global(cx.new(KeyStore::new), cx);
}

struct GlobalKeyStore(Entity<KeyStore>);

impl Global for GlobalKeyStore {}

#[derive(Debug)]
pub struct KeyStore {
    /// Local Keys for storage operations
    pub keys: Option<Arc<dyn NostrSigner>>,

    /// Indicates whether the secret service is missing
    pub secret_service_missing: bool,

    /// Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl KeyStore {
    /// Retrieve the global key store instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalKeyStore>().0.clone()
    }

    /// Retrieve the key store instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalKeyStore>().0.read(cx)
    }

    /// Set the global key store instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalKeyStore(state));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let read_credential = cx.read_credentials(KEYRING_URL);
        let mut tasks = smallvec![];

        tasks.push(
            // Load the stored credentials from the keyring
            cx.spawn(async move |this, cx| {
                let result = read_credential.await;

                this.update(cx, |this, cx| {
                    match result {
                        Ok(Some((_, secret))) => {
                            if let Ok(secret) = SecretKey::from_slice(&secret) {
                                this.keys = Some(Arc::new(Keys::new(secret)));
                            } else {
                                this.keys = None;
                            }
                        }
                        Ok(None) => {
                            this.keys = None;
                        }
                        Err(e) => {
                            log::error!("Secret Service: {e}");
                            this.secret_service_missing = true;
                        }
                    };
                    cx.notify();
                })
                .ok();
            }),
        );

        Self {
            keys: None,
            secret_service_missing: false,
            _tasks: tasks,
        }
    }

    /// Load the keyring-stored keys if available
    pub fn read_credential(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read_credential = cx.read_credentials(KEYRING_URL);

        cx.spawn_in(window, async move |this, cx| {
            let result = read_credential.await;

            this.update(cx, |this, cx| {
                match result {
                    Ok(Some((_, secret))) => {
                        if let Ok(secret) = SecretKey::from_slice(&secret) {
                            this.keys = Some(Arc::new(Keys::new(secret)));
                        } else {
                            this.keys = None;
                        }
                    }
                    Ok(None) => {
                        this.keys = None;
                    }
                    Err(e) => {
                        log::error!("Secret Service: {e}");
                        this.secret_service_missing = true;
                    }
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Store the value associated with the given key in the key store
    ///
    /// Encrypt with the keyring-stored keys if available
    pub fn store(&self, key: KeyItem, value: String, cx: &App) -> Task<Result<(), Error>> {
        let keyring = self.keys.as_ref().cloned();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let mut content = value.to_string();

            // Encrypt the value if the keyring is available
            if let Some(keys) = keyring.as_ref() {
                content = keys.nip44_encrypt(&public_key, value.as_ref()).await?;
            }

            // Construct the application data event
            let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
                .tag(Tag::identifier(key))
                .build(public_key)
                .sign(&Keys::generate())
                .await?;

            // Save the event to the database
            client.database().save_event(&event).await?;

            Ok(())
        })
    }

    /// Load the value associated with the given key from the key store
    ///
    /// Decrypt with the keyring-stored keys if available
    pub fn load(&self, key: KeyItem, cx: &App) -> Task<Result<String, Error>> {
        let keyring = self.keys.as_ref().cloned();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(key)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                if let Some(keys) = keyring {
                    let content = keys.nip44_decrypt(&public_key, &event.content).await?;
                    Ok(content)
                } else if Self::is_base64(&event.content) {
                    Err(anyhow!("Keyring is required"))
                } else {
                    Ok(event.content)
                }
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    /// Load the value associated with the given key from the key store
    ///
    /// Decrypt with the keyring-stored keys if available
    pub fn load_key_and_author(
        &self,
        key: KeyItem,
        cx: &App,
    ) -> Task<Result<(PublicKey, String), Error>> {
        let keyring = self.keys.as_ref().cloned();

        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(key)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                if let Some(keys) = keyring {
                    let content = keys.nip44_decrypt(&public_key, &event.content).await?;
                    Ok((event.pubkey, content))
                } else if Self::is_base64(&event.content) {
                    Err(anyhow!("Keyring is required"))
                } else {
                    Ok((event.pubkey, event.content))
                }
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    /// Check if the given string is a valid base64 string
    fn is_base64(s: &str) -> bool {
        use base64::engine::general_purpose;
        general_purpose::STANDARD.decode(s).is_ok()
    }
}
