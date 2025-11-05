use std::time::Duration;

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
        let mut tasks = smallvec![];

        tasks.push(
            // Handle notifications
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move { Self::observe_signer(&client).await })
                    .await;

                if let Some(public_key) = result {
                    this.update(cx, |this, cx| {
                        let client = nostr.read(cx).client();
                        // Set public key
                        this.public_key = Some(public_key);

                        // Get gossip relays
                        this._tasks.push(cx.background_spawn(async move {
                            Self::get_gossip_relays(&client, public_key).await.ok();
                        }));

                        cx.notify();
                    })
                    .expect("Entity has been released")
                }
            }),
        );

        Self {
            public_key: None,
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
            .subscribe_to(BOOTSTRAP_RELAYS, filter.clone(), Some(opts))
            .await?;

        Ok(())
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
