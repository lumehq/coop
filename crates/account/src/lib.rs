use std::collections::HashSet;
use std::time::Duration;

use anyhow::Error;
use common::BOOTSTRAP_RELAYS;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::{client, GIFTWRAP_SUBSCRIPTION};

pub fn init(cx: &mut App) {
    Account::set_global(cx.new(Account::new), cx);
}

struct GlobalAccount(Entity<Account>);

impl Global for GlobalAccount {}

/// Account
pub struct Account {
    /// The public key of the account
    public_key: Option<PublicKey>,

    /// Status of the current user NIP-65 relays
    nip65_status: Entity<RelayStatus>,

    /// Status of the current user NIP-17 relays
    nip17_status: Entity<RelayStatus>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 2]>,

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
        let nip65_status = cx.new(|_| RelayStatus::default());
        let nip17_status = cx.new(|_| RelayStatus::default());

        let mut tasks = smallvec![];
        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Observe the current entity
            cx.observe_self(move |this, cx| {
                if this.has_account() {
                    this.get_relay_list(cx);
                }
            }),
        );

        subscriptions.push(
            // Observe the nip65 relay status
            cx.observe(&nip65_status, move |this, state, cx| {
                if state.read(cx) == &RelayStatus::Set {
                    this.get_inbox_relay(cx);
                }
            }),
        );

        tasks.push(
            // Observe the nostr signer and set the public key when it sets
            cx.spawn(async move |this, cx| {
                // Observe the signer and return the public key when it sets
                let result = cx
                    .background_executor()
                    .await_on_background(async move {
                        let client = client();
                        let loop_duration = Duration::from_millis(800);

                        loop {
                            if let Ok(signer) = client.signer().await {
                                if let Ok(public_key) = signer.get_public_key().await {
                                    return Some(public_key);
                                }
                            }
                            smol::Timer::after(loop_duration).await;
                        }
                    })
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
            nip65_status,
            nip17_status,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    /// Get the metadata for a given public key
    async fn get_metadata(public_key: PublicKey) -> Result<(), Error> {
        let client = client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        // Construct filter for contact list
        let contacts = Filter::new()
            .kind(Kind::ContactList)
            .author(public_key)
            .limit(1);

        // Construct filter for profile
        let profile = Filter::new()
            .kind(Kind::Metadata)
            .author(public_key)
            .limit(1);

        client
            .subscribe(vec![contacts, profile], Some(opts))
            .await?;

        Ok(())
    }

    /// Get messages for a given public key
    async fn get_messages(public_key: PublicKey) -> Result<(), Error> {
        let client = client();
        let id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        client.subscribe_with_id(id, vec![filter], None).await?;

        Ok(())
    }

    /// Get relay list for a given public key
    async fn get_relay_list_for(public_key: PublicKey) -> Result<RelayStatus, Error> {
        let client = client();

        // Construct filter for NIP-65 relays
        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        let mut processed_events = HashSet::new();

        let mut stream = client
            .stream_events_from(BOOTSTRAP_RELAYS, vec![filter], Duration::from_secs(3))
            .await?;

        while let Some((_url, res)) = stream.next().await {
            let event = res?;

            // Skip if the event has already been processed
            if !processed_events.insert(event.id) {
                continue;
            }

            // Check if the event is authored by the current user
            if event.pubkey == public_key {
                Self::get_metadata(public_key).await?;
                return Ok(RelayStatus::Set);
            }
        }

        log::error!("Failed to get relay list for current user");

        Ok(RelayStatus::NotSet)
    }

    /// Get inbox relays for a given public key
    async fn get_inbox_relay_for(public_key: PublicKey) -> Result<RelayStatus, Error> {
        let client = client();

        // Construct filter for NIP-65 relays
        let filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(public_key)
            .limit(1);

        let mut processed_events = HashSet::new();

        let mut stream = client
            .stream_events(vec![filter], Duration::from_secs(3))
            .await?;

        while let Some((_url, res)) = stream.next().await {
            let event = res?;

            // Skip if the event has already been processed
            if !processed_events.insert(event.id) {
                continue;
            }

            // Check if the event is authored by the current user
            if event.pubkey == public_key {
                Self::get_messages(public_key).await?;
                return Ok(RelayStatus::Set);
            }
        }

        log::error!("Failed to get inbox relay for current user");

        Ok(RelayStatus::NotSet)
    }

    /// Get relay list for current user and update the status
    fn get_relay_list(&mut self, cx: &mut Context<Self>) {
        let public_key = self.public_key();

        self._tasks.push(
            // Run in the background thread
            cx.spawn(async move |this, cx| {
                let result: Result<RelayStatus, Error> = cx
                    .background_executor()
                    .await_on_background(async move { Self::get_relay_list_for(public_key).await })
                    .await;

                this.update(cx, |this, cx| {
                    match result {
                        Ok(status) => {
                            this.nip65_status.update(cx, |this, cx| {
                                *this = status;
                                cx.notify();
                            });
                        }
                        Err(e) => {
                            this.nip65_status.update(cx, |this, cx| {
                                *this = RelayStatus::NotSet;
                                cx.notify();
                            });
                            log::error!("Error: {e}");
                        }
                    };
                })
                .ok();
            }),
        );
    }

    /// Get inbox relays for current user and update the status
    fn get_inbox_relay(&mut self, cx: &mut Context<Self>) {
        let public_key = self.public_key();

        self._tasks.push(
            // Run in the background thread
            cx.spawn(async move |this, cx| {
                let result: Result<RelayStatus, Error> = cx
                    .background_executor()
                    .await_on_background(async move { Self::get_inbox_relay_for(public_key).await })
                    .await;

                this.update(cx, |this, cx| {
                    match result {
                        Ok(status) => {
                            this.nip17_status.update(cx, |this, cx| {
                                *this = status;
                                cx.notify();
                            });
                        }
                        Err(e) => {
                            this.nip17_status.update(cx, |this, cx| {
                                *this = RelayStatus::NotSet;
                                cx.notify();
                            });
                            log::error!("Error: {e}");
                        }
                    };
                })
                .ok();
            }),
        );
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
}
