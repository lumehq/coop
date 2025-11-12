use std::time::Duration;

use anyhow::Error;
use common::BOOTSTRAP_RELAYS;
use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;

pub fn init(cx: &mut App) {
    Account::set_global(cx.new(Account::new), cx);
}

struct GlobalAccount(Entity<Account>);

impl Global for GlobalAccount {}

pub struct Account {
    /// The public key of the account
    public_key: Option<PublicKey>,

    /// Status of the current user NIP-65 relays
    pub nip65_status: RelayStatus,

    /// Status of the current user NIP-17 relays
    pub nip17_status: RelayStatus,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelayStatus {
    #[default]
    Initial,
    NotSet,
    Set,
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

        let mut tasks = smallvec![];

        tasks.push(
            // Observe the nostr signer and set the public key when it sets
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move { Self::observe_signer(&client).await })
                    .await;

                if let Some(public_key) = result {
                    this.update(cx, |this, cx| {
                        this.set_account(public_key, cx);
                    })
                    .expect("Entity has been released")
                }
            }),
        );

        Self {
            public_key: None,
            nip65_status: RelayStatus::default(),
            nip17_status: RelayStatus::default(),
            _tasks: tasks,
        }
    }

    /// Observe the signer and return the public key when it sets
    async fn observe_signer(client: &Client) -> Option<PublicKey> {
        let loop_duration = Duration::from_millis(800);

        loop {
            if let Ok(signer) = client.signer().await {
                if let Ok(public_key) = signer.get_public_key().await {
                    // Get current user's gossip relays
                    Self::get_gossip_relays(client, public_key).await.ok()?;

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

    /// Ensure the user has NIP-65 relays
    async fn ensure_nip65_relays(client: &Client, public_key: PublicKey) -> Result<bool, Error> {
        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        // Count the number of nip65 relays event in the database
        let total = client.database().count(filter).await.unwrap_or(0);

        Ok(total > 0)
    }

    /// Ensure the user has NIP-17 relays
    async fn ensure_nip17_relays(client: &Client, public_key: PublicKey) -> Result<bool, Error> {
        let filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(public_key)
            .limit(1);

        // Count the number of nip17 relays event in the database
        let total = client.database().count(filter).await.unwrap_or(0);

        Ok(total > 0)
    }

    /// Set the public key of the account
    pub fn set_account(&mut self, public_key: PublicKey, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Update account's public key
        self.public_key = Some(public_key);

        // Add background task
        self._tasks.push(
            // Verify user's nip65 and nip17 relays
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(10))
                    .await;

                let ensure_nip65 = Self::ensure_nip65_relays(&client, public_key).await;
                let ensure_nip17 = Self::ensure_nip17_relays(&client, public_key).await;

                this.update(cx, |this, cx| {
                    this.nip65_status = match ensure_nip65 {
                        Ok(true) => RelayStatus::Set,
                        _ => RelayStatus::NotSet,
                    };
                    this.nip17_status = match ensure_nip17 {
                        Ok(true) => RelayStatus::Set,
                        _ => RelayStatus::NotSet,
                    };
                    cx.notify();
                })
                .expect("Entity has been released")
            }),
        );

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
}
