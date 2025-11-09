use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use account::Account;
use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::app_name;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
pub use signer::*;
use smallvec::{smallvec, SmallVec};
use state::{Announcement, NostrRegistry, Response};

mod signer;

pub fn init(cx: &mut App) {
    Encryption::set_global(cx.new(Encryption::new), cx);
}

struct GlobalEncryption(Entity<Encryption>);

impl Global for GlobalEncryption {}

pub struct Encryption {
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Client Signer that used for communication between devices
    client_signer: Entity<Option<Arc<dyn NostrSigner>>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption Key used for encryption and decryption instead of the user's identity
    encryption: Option<Arc<dyn NostrSigner>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption Key announcement
    announcement: Option<Arc<Announcement>>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Tasks for asynchronous operations
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
        let account = Account::global(cx);
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let client_signer: Entity<Option<Arc<dyn NostrSigner>>> = cx.new(|_| None);

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the account state
            cx.observe(&account, |this, state, cx| {
                if state.read(cx).has_account() && !this.has_encryption() {
                    this.get_announcement(cx);
                }
            }),
        );

        tasks.push(
            // Get the client key
            cx.spawn(async move |this, cx| {
                match Self::get_keys(&client, "client").await {
                    Ok(keys) => {
                        this.update(cx, |this, cx| {
                            this.set_client(Arc::new(keys), cx);
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
                            this.set_client(Arc::new(keys), cx);
                        })
                        .expect("Entity has been released");
                    }
                }
            }),
        );

        Self {
            client_signer,
            encryption: None,
            announcement: None,
            _subscriptions: subscriptions,
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

    fn get_announcement(&mut self, cx: &mut Context<Self>) {
        let task = self._get_announcement(cx);

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;

            match task.await {
                Ok(announcement) => {
                    this.update(cx, |this, cx| {
                        this.load_encryption(&announcement, cx);
                        // Set the announcement
                        this.announcement = Some(Arc::new(announcement));
                        cx.notify();
                    })
                    .expect("Entity has been released");
                }
                Err(err) => {
                    log::error!("Failed to get announcement: {}", err);
                }
            };
        })
        .detach();
    }

    fn _get_announcement(&self, cx: &App) -> Task<Result<Announcement, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        cx.background_spawn(async move {
            let user_signer = client.signer().await?;
            let public_key = user_signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::Custom(10044))
                .author(public_key)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first() {
                Ok(Self::extract_announcement(event)?)
            } else {
                Err(anyhow!("Announcement not found"))
            }
        })
    }

    /// Load the encryption key that stored in the database
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    fn load_encryption(&mut self, announcement: &Announcement, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let n = announcement.public_key();

        cx.spawn(async move |this, cx| {
            let result = Self::get_keys(&client, "encryption").await;

            this.update(cx, |this, cx| {
                if let Ok(keys) = result {
                    if keys.public_key() == n {
                        this.set_encryption(Arc::new(keys), cx);
                    }
                }
            })
            .expect("Entity has been released");
        })
        .detach();
    }

    /// Overwrite the encryption key
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub fn new_encryption(&self, cx: &App) -> Task<Result<Keys, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let keys = Keys::generate();
        let public_key = keys.public_key();
        let secret = keys.secret_key().to_secret_hex();

        // Create a task announce the encryption key
        cx.background_spawn(async move {
            // Store the encryption key to the database
            Self::set_keys(&client, "encryption", secret).await?;

            let signer = client.signer().await?;

            // Construct the announcement event
            let event = EventBuilder::new(Kind::Custom(10044), "")
                .tags(vec![
                    Tag::client(app_name()),
                    Tag::custom(TagKind::custom("n"), vec![public_key]),
                ])
                .sign(&signer)
                .await?;

            // Send the announcement event to user's relays
            client.send_event(&event).await?;

            Ok(keys)
        })
    }

    /// Send a request for encryption key from other clients
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub fn send_request(&self, cx: &App) -> Task<Result<Option<Keys>, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Get the client signer
        let Some(client_signer) = self.client_signer.read(cx).clone() else {
            return Task::ready(Err(anyhow!("Client Signer is required")));
        };

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let client_pubkey = client_signer.get_public_key().await?;

            // Get the encryption key approval response from the database first
            let filter = Filter::new()
                .kind(Kind::Custom(4455))
                .author(public_key)
                .pubkey(client_pubkey)
                .limit(1);

            match client.database().query(filter).await?.first_owned() {
                Some(event) => {
                    let root_device = event
                        .tags
                        .find(TagKind::custom("P"))
                        .and_then(|tag| tag.content())
                        .and_then(|content| PublicKey::parse(content).ok())
                        .context("Invalid event's tags")?;

                    let payload = event.content.as_str();
                    let decrypted = client_signer.nip44_decrypt(&root_device, payload).await?;

                    let secret = SecretKey::from_hex(&decrypted)?;
                    let keys = Keys::new(secret);

                    Ok(Some(keys))
                }
                None => {
                    // Construct encryption keys request event
                    let event = EventBuilder::new(Kind::Custom(4454), "")
                        .tags(vec![
                            Tag::client(app_name()),
                            Tag::custom(TagKind::custom("pubkey"), vec![client_pubkey]),
                        ])
                        .sign(&signer)
                        .await?;

                    // Send a request for encryption keys from other devices
                    client.send_event(&event).await?;

                    // Create a unique ID to control the subscription later
                    let subscription_id = SubscriptionId::new("encryption-request");

                    let filter = Filter::new()
                        .kind(Kind::Custom(4455))
                        .author(public_key)
                        .pubkey(client_pubkey)
                        .since(Timestamp::now());

                    // Subscribe to the approval response event
                    client
                        .subscribe_with_id(subscription_id, filter, None)
                        .await?;

                    Ok(None)
                }
            }
        })
    }

    /// Wait for the approval response event
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub fn wait_for_approval(&self, cx: &App) -> Task<Result<Keys, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let client_signer = self.client_signer.read(cx).clone().unwrap();
        let mut processed_events = HashSet::new();

        cx.background_spawn(async move {
            let mut notifications = client.notifications();
            log::info!("Listening for notifications");

            while let Ok(notification) = notifications.recv().await {
                let RelayPoolNotification::Message { message, .. } = notification else {
                    // Skip non-message notifications
                    continue;
                };

                if let RelayMessage::Event { event, .. } = message {
                    if !processed_events.insert(event.id) {
                        // Skip if the event has already been processed
                        continue;
                    }

                    if event.kind != Kind::Custom(4455) {
                        // Skip non-gift wrap events
                        continue;
                    }

                    if let Ok(response) = Self::extract_response(&client, &event).await {
                        let public_key = response.public_key();
                        let payload = response.payload();

                        // Decrypt the payload using the client signer
                        let decrypted = client_signer.nip44_decrypt(&public_key, payload).await?;
                        let secret = SecretKey::parse(&decrypted)?;
                        // Construct the encryption keys
                        let keys = Keys::new(secret);

                        return Ok(keys);
                    }
                }
            }

            Err(anyhow!("Failed to handle Encryption Key approval response"))
        })
    }

    /// Set the client signer for the account
    pub fn set_client(&mut self, signer: Arc<dyn NostrSigner>, cx: &mut Context<Self>) {
        self.client_signer.update(cx, |this, cx| {
            *this = Some(signer);
            cx.notify();
        });
    }

    /// Set the encryption signer for the account
    pub fn set_encryption(&mut self, signer: Arc<dyn NostrSigner>, cx: &mut Context<Self>) {
        self.encryption = Some(signer);
        cx.notify();
    }

    /// Check if the account entity has an encryption key
    pub fn has_encryption(&self) -> bool {
        self.encryption.is_some()
    }

    /// Returns the encryption announcement
    pub fn announcement(&self) -> Option<Arc<Announcement>> {
        self.announcement.clone()
    }

    /// Extract an encryption keys announcement from an event.
    fn extract_announcement(event: &Event) -> Result<Announcement, Error> {
        let public_key = event
            .tags
            .iter()
            .find(|tag| tag.kind().as_str() == "n" || tag.kind().as_str() == "pubkey")
            .and_then(|tag| tag.content())
            .and_then(|c| PublicKey::parse(c).ok())
            .context("Cannot parse public key from the event's tags")?;

        let client_name = event
            .tags
            .find(TagKind::Client)
            .and_then(|tag| tag.content())
            .map(|c| c.to_string())
            .context("Cannot parse client name from the event's tags")?;

        Ok(Announcement::new(event.id, client_name, public_key))
    }

    /// Extract an encryption keys response from an event.
    async fn extract_response(client: &Client, event: &Event) -> Result<Response, Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        if event.pubkey != public_key {
            return Err(anyhow!("Event does not belong to current user"));
        }

        let client_pubkey = event
            .tags
            .find(TagKind::custom("P"))
            .and_then(|tag| tag.content())
            .and_then(|c| PublicKey::parse(c).ok())
            .context("Cannot parse public key from the event's tags")?;

        Ok(Response::new(event.content.clone(), client_pubkey))
    }
}
