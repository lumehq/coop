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
use state::{Announcement, NostrRegistry};

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
    pub encryption: Entity<Option<Arc<dyn NostrSigner>>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption Key announcement
    announcement: Option<Arc<Announcement>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Requests for encryption keys from other devices
    requests: Entity<HashSet<Announcement>>,

    /// Async task for handling notifications
    handle_notifications: Option<Task<()>>,

    /// Async task for handling requests
    handle_requests: Option<Task<()>>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 2]>,
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

        let requests = cx.new(|_| HashSet::default());
        let encryption = cx.new(|_| None);
        let client_signer = cx.new(|_| None);

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the account state
            cx.observe(&account, |this, state, cx| {
                if state.read(cx).has_account() && !this.has_encryption(cx) {
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
            requests,
            client_signer,
            encryption,
            announcement: None,
            handle_notifications: None,
            handle_requests: None,
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

        self._tasks.push(cx.spawn(async move |this, cx| {
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
        }));
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
                Ok(NostrRegistry::extract_announcement(event)?)
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
                        this.listen_request(cx);
                    }
                }
            })
            .expect("Entity has been released");
        })
        .detach();
    }

    /// Listen for the encryption key request from other devices
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub fn listen_request(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let (tx, rx) = flume::bounded::<Announcement>(50);

        let task: Task<Result<(), Error>> = cx.background_spawn({
            let client = Arc::clone(&client);

            async move {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let id = SubscriptionId::new("listen-request");

                let filter = Filter::new()
                    .author(public_key)
                    .kind(Kind::Custom(4454))
                    .since(Timestamp::now());

                // Unsubscribe from the previous subscription
                client.unsubscribe(&id).await;

                // Subscribe to the new subscription
                client.subscribe_with_id(id, filter, None).await?;

                Ok(())
            }
        });

        // Run this task and finish in the background
        task.detach();

        // Handle notifications
        self.handle_notifications = Some(cx.background_spawn(async move {
            let mut notifications = client.notifications();
            let mut processed_events = HashSet::new();

            while let Ok(notification) = notifications.recv().await {
                let RelayPoolNotification::Message { message, .. } = notification else {
                    // Skip if the notification is not a message
                    continue;
                };

                if let RelayMessage::Event { event, .. } = message {
                    if !processed_events.insert(event.id) {
                        // Skip if the event has already been processed
                        continue;
                    }

                    if event.kind != Kind::Custom(4454) {
                        // Skip if the event is not a encryption events
                        continue;
                    };

                    if NostrRegistry::is_self_authored(&client, &event).await {
                        if let Ok(announcement) = NostrRegistry::extract_announcement(&event) {
                            tx.send_async(announcement).await.ok();
                        }
                    }
                }
            }
        }));

        // Handle requests
        self.handle_requests = Some(cx.spawn(async move |this, cx| {
            while let Ok(request) = rx.recv_async().await {
                this.update(cx, |this, cx| {
                    this.set_request(request, cx);
                })
                .expect("Entity has been released");
            }
        }));
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
                    let subscription_id = SubscriptionId::new("listen-response");

                    let filter = Filter::new()
                        .kind(Kind::Custom(4455))
                        .author(public_key)
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

    /// Send the approval response event
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub fn send_response(&self, target: PublicKey, cx: &App) -> Task<Result<(), Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Get the client signer
        let Some(client_signer) = self.client_signer.read(cx).clone() else {
            return Task::ready(Err(anyhow!("Client Signer is required")));
        };

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let encryption = Self::get_keys(&client, "encryption").await?;
            let client_pubkey = client_signer.get_public_key().await?;

            // Encrypt the encryption keys with the client's signer
            let payload = client_signer
                .nip44_encrypt(&target, &encryption.secret_key().to_secret_hex())
                .await?;

            // Construct the response event
            //
            // P tag: the current client's public key
            // p tag: the requester's public key
            let event = EventBuilder::new(Kind::Custom(4455), payload)
                .tags(vec![
                    Tag::custom(TagKind::custom("P"), vec![client_pubkey]),
                    Tag::public_key(target),
                ])
                .sign(&signer)
                .await?;

            // Send the response event to the user's relay list
            client.send_event(&event).await?;

            Ok(())
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

                    if let Ok(response) = NostrRegistry::extract_response(&client, &event).await {
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
        self.encryption.update(cx, |this, cx| {
            *this = Some(signer);
            cx.notify();
        });
    }

    /// Check if the account entity has an encryption key
    pub fn has_encryption(&self, cx: &App) -> bool {
        self.encryption.read(cx).is_some()
    }

    /// Returns the encryption key
    pub fn encryption_key(&self, cx: &App) -> Option<Arc<dyn NostrSigner>> {
        self.encryption.read(cx).clone()
    }

    /// Returns the encryption announcement
    pub fn announcement(&self) -> Option<Arc<Announcement>> {
        self.announcement.clone()
    }

    /// Returns the encryption requests
    pub fn requests(&self) -> Entity<HashSet<Announcement>> {
        self.requests.clone()
    }

    /// Push the encryption request
    pub fn set_request(&mut self, request: Announcement, cx: &mut Context<Self>) {
        self.requests.update(cx, |this, cx| {
            this.insert(request);
            cx.notify();
        });
    }
}
