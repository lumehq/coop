use std::time::Duration;

use anyhow::Error;
use global::constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID};
use global::shared_state;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task, Window};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use ui::notification::Notification;
use ui::ContextModal;

struct GlobalAppState(Entity<AppState>);

impl Global for GlobalAppState {}

pub fn init(cx: &mut App) {
    AppState::set_global(cx.new(AppState::new), cx);
}

pub struct AppState {
    account: Option<Profile>,
    client_keys: Option<Keys>,
    /// Subscriptions for observing changes
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl AppState {
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAppState>().0.clone()
    }

    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalAppState>().0.read(cx)
    }

    pub(crate) fn set_global(account: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAppState(account));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let mut subscriptions = smallvec![];

        subscriptions.push(cx.observe_new::<AppState>(|_this, window, cx| {
            if let Some(window) = window {
                let client_keys_task = cx.read_credentials("coop_client");

                cx.spawn_in(window, async move |this, cx| {
                    if let Ok(Some((_, secret))) = client_keys_task.await {
                        this.update(cx, |this, cx| {
                            let keys = SecretKey::from_slice(&secret).map(Keys::new).ok();
                            this.set_client_keys(keys, cx);
                        })
                        .ok();
                    } else {
                        this.update(cx, |this, cx| {
                            this.generate_new_keys(cx);
                        })
                        .ok();
                    };
                })
                .detach();
            }
        }));

        Self {
            account: None,
            client_keys: None,
            subscriptions,
        }
    }

    /// Subscribes to the current account's metadata.
    fn subscribe(&self, cx: &mut Context<Self>) {
        let Some(public_key) = self.account.as_ref().map(|this| this.public_key()) else {
            return;
        };

        let metadata = Filter::new()
            .kinds(vec![
                Kind::Metadata,
                Kind::ContactList,
                Kind::InboxRelays,
                Kind::MuteList,
                Kind::SimpleGroups,
            ])
            .author(public_key)
            .limit(10);

        let data = Filter::new()
            .author(public_key)
            .kinds(vec![
                Kind::Metadata,
                Kind::ContactList,
                Kind::MuteList,
                Kind::SimpleGroups,
                Kind::InboxRelays,
                Kind::RelayList,
            ])
            .since(Timestamp::now());

        let msg = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
        let new_msg = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(public_key)
            .limit(0);

        let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
        let new_messages_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

        cx.background_spawn(async move {
            let client = &shared_state().client;
            let opts = shared_state().auto_close;

            client.subscribe(data, None).await.ok();

            client
                .subscribe(metadata, shared_state().auto_close)
                .await
                .ok();

            client
                .subscribe_with_id(all_messages_sub_id, msg, opts)
                .await
                .ok();

            client
                .subscribe_with_id(new_messages_sub_id, new_msg, None)
                .await
                .ok();
        })
        .detach();
    }

    /// Login to the account using the given signer.
    pub fn login<S>(&mut self, signer: S, window: &mut Window, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        let task: Task<Result<Profile, Error>> = cx.background_spawn(async move {
            let public_key = signer.get_public_key().await?;

            // Update signer
            shared_state().client.set_signer(signer).await;

            // Fetch user's metadata
            let metadata = shared_state()
                .client
                .fetch_metadata(public_key, Duration::from_secs(2))
                .await?
                .unwrap_or_default();

            Ok(Profile::new(public_key, metadata))
        });

        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(profile) => {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_account(profile, cx);
                        // Start subscription for this account
                        cx.defer_in(window, |this, _, cx| {
                            this.subscribe(cx);
                        });
                    })
                    .ok();
                })
                .ok();
            }
            Err(e) => {
                cx.update(|window, cx| {
                    window.push_notification(Notification::error(e.to_string()), cx)
                })
                .ok();
            }
        })
        .detach();
    }

    /// Create a new account with the given metadata.

    /// Set the current account's profile.
    pub fn set_account(&mut self, profile: Profile, cx: &mut Context<Self>) {
        self.account = Some(profile);
        cx.notify();
    }

    /// Get the reference to account's profile.
    pub fn account(&self) -> Option<&Profile> {
        self.account.as_ref()
    }

    /// Set the client keys.
    pub fn set_client_keys(&mut self, keys: Option<Keys>, cx: &mut Context<Self>) {
        self.client_keys = keys;
        cx.notify();
    }

    /// Set the client keys.
    pub fn generate_new_keys(&mut self, cx: &mut Context<Self>) {
        let keys = Keys::generate();
        let save_keys = cx.write_credentials(
            "coop_client",
            &keys.public_key.to_hex(),
            keys.secret_key().as_secret_bytes(),
        );

        // Update the client keys
        self.set_client_keys(Some(keys), cx);

        // Save keys in the background
        cx.background_spawn(async move {
            if let Err(e) = save_keys.await {
                log::error!("Failed to save keys: {}", e);
            }
        })
        .detach();
    }

    /// Get the reference to the client keys.
    pub fn client_keys(&self) -> Option<&Keys> {
        self.client_keys.as_ref()
    }
}
