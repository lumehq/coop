use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::BOOTSTRAP_RELAYS;
pub use encryption::*;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::{Announcement, NostrRegistry};

mod encryption;

pub fn init(cx: &mut App) {
    Account::set_global(cx.new(Account::new), cx);
}

struct GlobalAccount(Entity<Account>);

impl Global for GlobalAccount {}

pub struct Account {
    /// The public key of the account
    public_key: Option<PublicKey>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Client Signer that used for communication between devices
    client_signer: Entity<Option<Arc<dyn NostrSigner>>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption Key used for encryption and decryption instead of the user's identity
    encryption: Option<Arc<dyn NostrSigner>>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 2]>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Account {
    /// Retrieve the global account state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAccount>().0.clone()
    }

    /// Check if the global account state exists
    pub fn has_global(cx: &App) -> bool {
        cx.has_global::<GlobalAccount>()
    }

    /// Remove the global account state
    pub fn remove_global(cx: &mut App) {
        cx.remove_global::<GlobalAccount>();
    }

    /// Set the global account instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAccount(state));
    }

    /// Create a new account instance
    fn new(cx: &mut Context<Self>) -> Self {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let client_signer: Entity<Option<Arc<dyn NostrSigner>>> = cx.new(|_| None);

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the client signer state
            cx.observe(&client_signer, move |this, state, cx| {
                if let Some(signer) = state.read(cx).clone() {
                    if this.encryption.is_none() {
                        this.get_encryption(&signer, cx);
                    }
                }
            }),
        );

        subscriptions.push(
            // Observe when the public key is set
            cx.observe_self(move |this, cx| {
                let client = nostr.read(cx).client();

                if let Some(public_key) = this.public_key {
                    this._tasks.push(
                        // Get current user's gossip relays
                        cx.background_spawn({
                            let client = Arc::clone(&client);
                            async move {
                                Self::get_gossip_relays(&client, public_key).await.ok();
                            }
                        }),
                    );

                    this._tasks.push(
                        // Initialize the client key
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
                }
            }),
        );

        tasks.push(
            // Handle notifications
            cx.spawn({
                let client = Arc::clone(&client);

                async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { Self::observe_signer(&client).await })
                        .await;

                    if let Some(public_key) = result {
                        this.update(cx, |this, cx| {
                            this.set_account(public_key, cx);
                        })
                        .expect("Entity has been released")
                    }
                }
            }),
        );

        Self {
            public_key: None,
            client_signer,
            encryption: None,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    /// Observe the signer and return the public key when it sets
    async fn observe_signer(client: &Client) -> Option<PublicKey> {
        let loop_duration = Duration::from_millis(800);

        loop {
            if let Ok(signer) = client.signer().await {
                if let Ok(public_key) = signer.get_public_key().await {
                    return Some(public_key);
                }
            }
            smol::Timer::after(loop_duration).await;
        }
    }

    /// Get gossip relays for a given public key
    async fn get_gossip_relays(client: &Client, public_key: PublicKey) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        // Subscribe to events from the bootstrapping relays
        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        log::info!("Getting user's gossip relays...");

        Ok(())
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

    fn get_encryption(&mut self, _signer: &Arc<dyn NostrSigner>, cx: &mut Context<Self>) {
        let task = self._get_encryption(cx);

        cx.spawn(async move |_this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;

            match task.await {
                Ok(announcement) => {
                    log::info!("Received encryption announcement: {announcement:?}");
                    // Handle the announcement
                }
                Err(e) => {
                    log::info!("Encryption error: {e}")
                    // Handle the error
                }
            }
        })
        .detach();
    }

    fn _get_encryption(&self, cx: &App) -> Task<Result<Announcement, Error>> {
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

    /// Set the public key of the account
    pub fn set_account(&mut self, public_key: PublicKey, cx: &mut Context<Self>) {
        self.public_key = Some(public_key);
        cx.notify();
    }

    /// Check if the account entity has a public key
    pub fn has_account(&self) -> bool {
        self.public_key.is_some()
    }

    /// Get the public key of the account
    pub fn public_key(&self) -> PublicKey {
        // This method is only called when user is logged in, so unwrap safely
        self.public_key.unwrap()
    }

    /// Set the client signer for the account
    pub fn set_client(&mut self, signer: Arc<dyn NostrSigner>, cx: &mut Context<Self>) {
        self.client_signer.update(cx, |this, cx| {
            *this = Some(signer);
            cx.notify();
        });
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
}
