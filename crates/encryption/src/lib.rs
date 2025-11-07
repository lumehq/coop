use std::sync::Arc;

use anyhow::{anyhow, Error};
use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;

pub fn init(cx: &mut App) {
    Encryption::set_global(cx.new(Encryption::new), cx);
}

struct GlobalEncryption(Entity<Encryption>);

impl Global for GlobalEncryption {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize, Serialize)]
pub enum SignerKind {
    Encryption,
    #[default]
    User,
    Auto,
}

#[derive(Debug)]
pub struct Encryption {
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Client Key that used for communication between devices
    pub client: Option<Arc<dyn NostrSigner>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption key used for encryption and decryption instead of the user's identity
    pub encryption: Option<Arc<dyn NostrSigner>>,

    /// Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Encryption {
    /// Retrieve the global encryption state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalEncryption>().0.clone()
    }

    /// Set the global encryption instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalEncryption(state));
    }

    /// Create a new encryption instance
    fn new(cx: &mut Context<Self>) -> Self {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let mut tasks = smallvec![];

        tasks.push(
            // Init the client key
            cx.spawn(async move |this, cx| {
                match Self::get_keys(&client, "client").await {
                    Ok(keys) => {
                        this.update(cx, |this, cx| {
                            this.client = Some(Arc::new(keys));
                            cx.notify();
                        })
                        .expect("Entity has been released");
                    }
                    Err(_) => {
                        let keys = Keys::generate();
                        let secret = keys.secret_key().to_secret_hex();

                        // Store the key in the database for future use
                        Self::set_keys(&client, "client", secret).await.ok();

                        // Update global state
                        this.update(cx, |this, cx| {
                            this.client = Some(Arc::new(keys));
                            cx.notify();
                        })
                        .expect("Entity has been released");
                    }
                }
            }),
        );

        Self {
            client: None,
            encryption: None,
            _tasks: tasks,
        }
    }

    /// Encrypt and store a key in the local database.
    async fn set_keys<T>(client: &Client, kind: T, value: String) -> Result<(), Error>
    where
        T: Into<String>,
    {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        // Encrypt the value
        let content = signer.nip44_encrypt(&public_key, value.as_ref()).await?;

        // Construct the application data event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
            .tag(Tag::identifier(format!("coop:{}", kind.into())))
            .build(public_key)
            .sign(&Keys::generate())
            .await?;

        // Save the event to the database
        client.database().save_event(&event).await?;

        Ok(())
    }

    /// Get and decrypt a key from the local database.
    async fn get_keys<T>(client: &Client, kind: T) -> Result<Keys, Error>
    where
        T: Into<String>,
    {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(format!("coop:{}", kind.into()));

        if let Some(event) = client.database().query(filter).await?.first() {
            let content = signer.nip44_decrypt(&public_key, &event.content).await?;
            let secret = SecretKey::parse(&content)?;
            let keys = Keys::new(secret);

            Ok(keys)
        } else {
            Err(anyhow!("Key not found"))
        }
    }
}
