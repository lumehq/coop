use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::app_name;
pub use device::*;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::{NostrRegistry, RelayState, GIFTWRAP_SUBSCRIPTION, TIMEOUT};

mod device;

const IDENTIFIER: &str = "coop:device";

pub fn init(cx: &mut App) {
    DeviceRegistry::set_global(cx.new(DeviceRegistry::new), cx);
}

struct GlobalDeviceRegistry(Entity<DeviceRegistry>);

impl Global for GlobalDeviceRegistry {}

/// Device Registry
#[derive(Debug)]
pub struct DeviceRegistry {
    /// Device signer
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    pub device_signer: Entity<Option<Arc<dyn NostrSigner>>>,

    /// Device requests
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    requests: Entity<HashSet<Event>>,

    /// Device state
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    state: DeviceState,

    /// Async tasks
    tasks: Vec<Task<Result<(), Error>>>,

    /// Subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,
}

impl DeviceRegistry {
    /// Retrieve the global device registry state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalDeviceRegistry>().0.clone()
    }

    /// Set the global device registry instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalDeviceRegistry(state));
    }

    /// Create a new device registry instance
    fn new(cx: &mut Context<Self>) -> Self {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let identity = nostr.read(cx).identity();

        let device_signer = cx.new(|_| None);
        let requests = cx.new(|_| HashSet::default());

        // Channel for communication between nostr and gpui
        let (tx, rx) = flume::bounded::<Event>(100);

        let mut subscriptions = smallvec![];
        let mut tasks = vec![];

        subscriptions.push(
            // Observe the identity entity
            cx.observe(&identity, |this, state, cx| {
                if state.read(cx).has_public_key() {
                    if state.read(cx).relay_list_state() == RelayState::Set {
                        this.get_announcement(cx);
                    }
                    if state.read(cx).messaging_relays_state() == RelayState::Set {
                        this.get_messages(cx);
                    }
                }
            }),
        );

        tasks.push(
            // Handle nostr notifications
            cx.background_spawn(async move { Self::handle_notifications(&client, &tx).await }),
        );

        tasks.push(
            // Update GPUI states
            cx.spawn(async move |this, cx| {
                while let Ok(event) = rx.recv_async().await {
                    match event.kind {
                        Kind::Custom(4454) => {
                            this.update(cx, |this, cx| {
                                this.add_request(event, cx);
                            })?;
                        }
                        Kind::Custom(4455) => {
                            this.update(cx, |this, cx| {
                                this.parse_response(event, cx);
                            })?;
                        }
                        _ => {}
                    }
                }

                Ok(())
            }),
        );

        Self {
            device_signer,
            requests,
            state: DeviceState::default(),
            tasks,
            _subscriptions: subscriptions,
        }
    }

    /// Handle nostr notifications
    async fn handle_notifications(client: &Client, tx: &flume::Sender<Event>) -> Result<(), Error> {
        let mut notifications = client.notifications();
        let mut processed_events = HashSet::new();

        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Message {
                message: RelayMessage::Event { event, .. },
                ..
            } = notification
            {
                if !processed_events.insert(event.id) {
                    // Skip if the event has already been processed
                    continue;
                }

                match event.kind {
                    Kind::Custom(4454) => {
                        if Self::verify_author(client, event.as_ref()).await {
                            tx.send_async(event.into_owned()).await.ok();
                        }
                    }
                    Kind::Custom(4455) => {
                        if Self::verify_author(client, event.as_ref()).await {
                            tx.send_async(event.into_owned()).await.ok();
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Verify the author of an event
    async fn verify_author(client: &Client, event: &Event) -> bool {
        if let Ok(signer) = client.signer().await {
            if let Ok(public_key) = signer.get_public_key().await {
                return public_key == event.pubkey;
            }
        }
        false
    }

    /// Encrypt and store device keys in the local database.
    async fn set_keys(client: &Client, secret: &str) -> Result<(), Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        // Encrypt the value
        let content = signer.nip44_encrypt(&public_key, secret).await?;

        // Construct the application data event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
            .tag(Tag::identifier(IDENTIFIER))
            .build(public_key)
            .sign(&Keys::generate())
            .await?;

        // Save the event to the database
        client.database().save_event(&event).await?;

        Ok(())
    }

    /// Get device keys from the local database.
    async fn get_keys(client: &Client) -> Result<Keys, Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(IDENTIFIER);

        if let Some(event) = client.database().query(filter).await?.first() {
            let content = signer.nip44_decrypt(&public_key, &event.content).await?;
            let secret = SecretKey::parse(&content)?;
            let keys = Keys::new(secret);

            Ok(keys)
        } else {
            Err(anyhow!("Key not found"))
        }
    }

    /// Returns the device signer entity
    pub fn signer(&self, cx: &App) -> Option<Arc<dyn NostrSigner>> {
        self.device_signer.read(cx).clone()
    }

    /// Set the decoupled encryption key for the current user
    fn set_device_signer<S>(&mut self, signer: S, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        self.set_state(DeviceState::Set, cx);
        self.device_signer.update(cx, |this, cx| {
            *this = Some(Arc::new(signer));
            cx.notify();
        });
    }

    /// Set the device state
    fn set_state(&mut self, state: DeviceState, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }

    /// Add a request for device keys
    fn add_request(&mut self, request: Event, cx: &mut Context<Self>) {
        self.requests.update(cx, |this, cx| {
            this.insert(request);
            cx.notify();
        });
    }

    /// Continuously get gift wrap events for the current user in their messaging relays
    fn get_messages(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let device_signer = self.device_signer.read(cx).clone();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let messaging_relays = nostr.read(cx).messaging_relays(&public_key, cx);

        cx.background_spawn(async move {
            let urls = messaging_relays.await;
            let id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);
            let mut filters = vec![];

            // Construct a filter to get user messages
            filters.push(Filter::new().kind(Kind::GiftWrap).pubkey(public_key));

            // Construct a filter to get dekey messages if available
            if let Some(signer) = device_signer.as_ref() {
                if let Ok(pubkey) = signer.get_public_key().await {
                    filters.push(Filter::new().kind(Kind::GiftWrap).pubkey(pubkey));
                }
            }

            if let Err(e) = client.subscribe_with_id_to(urls, id, filters, None).await {
                log::error!("Failed to subscribe to gift wrap events: {e}");
            }
        })
        .detach();
    }

    /// Get device announcement for current user
    fn get_announcement(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let write_relays = nostr.read(cx).write_relays(&public_key, cx);

        let task: Task<Result<Event, Error>> = cx.background_spawn(async move {
            let urls = write_relays.await;

            // Construct the filter for the device announcement event
            let filter = Filter::new()
                .kind(Kind::Custom(10044))
                .author(public_key)
                .limit(1);

            let mut stream = client
                .stream_events_from(&urls, vec![filter], Duration::from_secs(TIMEOUT))
                .await?;

            while let Some((_url, res)) = stream.next().await {
                match res {
                    Ok(event) => {
                        log::info!("Received device announcement event: {event:?}");
                        return Ok(event);
                    }
                    Err(e) => {
                        log::error!("Failed to receive device announcement event: {e}");
                    }
                }
            }

            Err(anyhow!("Device announcement not found"))
        });

        self.tasks.push(cx.spawn(async move |this, cx| {
            match task.await {
                Ok(event) => {
                    this.update(cx, |this, cx| {
                        this.init_device_signer(&event, cx);
                    })?;
                }
                Err(_) => {
                    this.update(cx, |this, cx| {
                        this.announce_device(cx);
                    })?;
                }
            }

            Ok(())
        }));
    }

    /// Create a new device signer and announce it
    fn announce_device(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let write_relays = nostr.read(cx).write_relays(&public_key, cx);

        let keys = Keys::generate();
        let secret = keys.secret_key().to_secret_hex();
        let n = keys.public_key();

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let signer = client.signer().await?;
            let urls = write_relays.await;

            // Construct an announcement event
            let event = EventBuilder::new(Kind::Custom(10044), "")
                .tags(vec![
                    Tag::custom(TagKind::custom("n"), vec![n]),
                    Tag::client(app_name()),
                ])
                .sign(&signer)
                .await?;

            // Publish announcement
            client.send_event_to(&urls, &event).await?;

            // Save device keys to the database
            Self::set_keys(&client, &secret).await?;

            Ok(())
        });

        cx.spawn(async move |this, cx| {
            if task.await.is_ok() {
                this.update(cx, |this, cx| {
                    this.set_device_signer(keys, cx);
                    this.listen_device_request(cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// Initialize device signer (decoupled encryption key) for the current user
    fn init_device_signer(&mut self, event: &Event, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let announcement = Announcement::from(event);
        let device_pubkey = announcement.public_key();

        let task: Task<Result<Keys, Error>> = cx.background_spawn(async move {
            if let Ok(keys) = Self::get_keys(&client).await {
                if keys.public_key() != device_pubkey {
                    return Err(anyhow!("Key mismatch"));
                };

                Ok(keys)
            } else {
                Err(anyhow!("Key not found"))
            }
        });

        cx.spawn(async move |this, cx| {
            match task.await {
                Ok(keys) => {
                    this.update(cx, |this, cx| {
                        this.set_device_signer(keys, cx);
                        this.listen_device_request(cx);
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.request_device_keys(cx);
                        this.listen_device_approval(cx);
                    })
                    .ok();

                    log::warn!("Failed to initialize device signer: {e}");
                }
            };
        })
        .detach();
    }

    /// Listen for device key requests on user's write relays
    fn listen_device_request(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let write_relays = nostr.read(cx).write_relays(&public_key, cx);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let urls = write_relays.await;

            // Construct a filter for device key requests
            let filter = Filter::new()
                .kind(Kind::Custom(4454))
                .author(public_key)
                .since(Timestamp::now());

            // Subscribe to the device key requests on user's write relays
            client.subscribe_to(&urls, vec![filter], None).await?;

            Ok(())
        });

        task.detach();
    }

    /// Listen for device key approvals on user's write relays
    fn listen_device_approval(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let write_relays = nostr.read(cx).write_relays(&public_key, cx);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let urls = write_relays.await;

            // Construct a filter for device key requests
            let filter = Filter::new()
                .kind(Kind::Custom(4455))
                .author(public_key)
                .since(Timestamp::now());

            // Subscribe to the device key requests on user's write relays
            client.subscribe_to(&urls, vec![filter], None).await?;

            Ok(())
        });

        task.detach();
    }

    /// Request encryption keys from other device
    fn request_device_keys(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let write_relays = nostr.read(cx).write_relays(&public_key, cx);

        let app_keys = nostr.read(cx).app_keys().clone();
        let app_pubkey = app_keys.public_key();

        let task: Task<Result<Option<Keys>, Error>> = cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::Custom(4455))
                .author(public_key)
                .pubkey(app_pubkey)
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
                    let decrypted = app_keys.nip44_decrypt(&root_device, payload).await?;

                    let secret = SecretKey::from_hex(&decrypted)?;
                    let keys = Keys::new(secret);

                    Ok(Some(keys))
                }
                None => {
                    let urls = write_relays.await;

                    // Construct an event for device key request
                    let event = EventBuilder::new(Kind::Custom(4454), "")
                        .tags(vec![
                            Tag::client(app_name()),
                            Tag::custom(TagKind::custom("P"), vec![app_pubkey]),
                        ])
                        .sign(&signer)
                        .await?;

                    // Send the event to write relays
                    client.send_event_to(&urls, &event).await?;

                    Ok(None)
                }
            }
        });

        cx.spawn(async move |this, cx| {
            match task.await {
                Ok(Some(keys)) => {
                    this.update(cx, |this, cx| {
                        this.set_device_signer(keys, cx);
                    })
                    .ok();
                }
                Ok(None) => {
                    this.update(cx, |this, cx| {
                        this.set_state(DeviceState::Requesting, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("Failed to request the encryption key: {e}");
                }
            };
        })
        .detach();
    }

    /// Parse the response event for device keys from other devices
    fn parse_response(&mut self, event: Event, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let app_keys = nostr.read(cx).app_keys().clone();

        let task: Task<Result<Keys, Error>> = cx.background_spawn(async move {
            let root_device = event
                .tags
                .find(TagKind::custom("P"))
                .and_then(|tag| tag.content())
                .and_then(|content| PublicKey::parse(content).ok())
                .context("Invalid event's tags")?;

            let payload = event.content.as_str();
            let decrypted = app_keys.nip44_decrypt(&root_device, payload).await?;

            let secret = SecretKey::from_hex(&decrypted)?;
            let keys = Keys::new(secret);

            Ok(keys)
        });

        cx.spawn(async move |this, cx| {
            match task.await {
                Ok(keys) => {
                    this.update(cx, |this, cx| {
                        this.set_device_signer(keys, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("Error: {e}")
                }
            };
        })
        .detach();
    }

    /// Approve requests for device keys from other devices
    #[allow(dead_code)]
    fn approve(&mut self, event: Event, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let public_key = nostr.read(cx).identity().read(cx).public_key();
        let write_relays = nostr.read(cx).write_relays(&public_key, cx);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let urls = write_relays.await;
            let signer = client.signer().await?;

            // Get device keys
            let keys = Self::get_keys(&client).await?;
            let secret = keys.secret_key().to_secret_hex();

            // Extract the target public key from the event tags
            let target = event
                .tags
                .find(TagKind::custom("P"))
                .and_then(|tag| tag.content())
                .and_then(|content| PublicKey::parse(content).ok())
                .context("Target is not a valid public key")?;

            // Encrypt the device keys with the user's signer
            let payload = signer.nip44_encrypt(&target, &secret).await?;

            // Construct the response event
            //
            // P tag: the current device's public key
            // p tag: the requester's public key
            let event = EventBuilder::new(Kind::Custom(4455), payload)
                .tags(vec![
                    Tag::custom(TagKind::custom("P"), vec![keys.public_key()]),
                    Tag::public_key(target),
                ])
                .sign(&signer)
                .await?;

            // Send the response event to the user's relay list
            client.send_event_to(&urls, &event).await?;

            Ok(())
        });

        task.detach();
    }
}
