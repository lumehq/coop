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

    /// Device state
    ///
    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    state: Entity<DeviceState>,

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
        let state = cx.new(|_| DeviceState::default());

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
                            //
                        }
                        Kind::Custom(4455) => {
                            //
                        }
                        _ => {}
                    }
                }

                Ok(())
            }),
        );

        Self {
            device_signer,
            state,
            tasks,
            _subscriptions: subscriptions,
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
        self.device_signer.update(cx, |this, cx| {
            *this = Some(Arc::new(signer));
            cx.notify();
        });
        self.state.update(cx, |this, cx| {
            *this = DeviceState::Set;
            cx.notify();
        });
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

            // Encrypt the secret key
            let encrypted = signer.nip44_encrypt(&public_key, &secret).await?;

            // Construct a storage event
            let event = EventBuilder::new(Kind::ApplicationSpecificData, encrypted)
                .tag(Tag::identifier("coop:device"))
                .sign(&signer)
                .await?;

            // Save storage event to database
            //
            // Note: never publish to any relays
            client.database().save_event(&event).await?;

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
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .identifier("coop:device")
                .kind(Kind::ApplicationSpecificData)
                .author(public_key)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first() {
                let content = signer.nip44_decrypt(&public_key, &event.content).await?;
                let secret = SecretKey::parse(&content)?;
                let keys = Keys::new(secret);

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
                        this.state.update(cx, |this, cx| {
                            *this = DeviceState::Requesting;
                            cx.notify();
                        });
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
}
