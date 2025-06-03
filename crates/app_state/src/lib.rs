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
                // let user_keys_task = cx.read_credentials("coop_user");
                let client_keys_task = cx.read_credentials("coop_client");

                cx.spawn_in(window, async move |this, cx| {
                    if let Ok(Some(task)) = client_keys_task.await {
                        let keys = SecretKey::from_slice(&task.1).map(Keys::new).ok();

                        this.update(cx, |this, cx| {
                            this.client_keys = keys;
                            cx.notify();
                        })
                        .ok();
                    } else {
                        let keys = Keys::generate();

                        this.update(cx, |this, cx| {
                            let save_keys_task = cx.write_credentials(
                                "coop_client",
                                &keys.public_key.to_hex(),
                                keys.secret_key().as_secret_bytes(),
                            );

                            cx.background_spawn(async move {
                                save_keys_task.await.ok();
                            })
                            .detach();

                            this.client_keys = Some(keys);
                            cx.notify();
                        })
                        .ok();
                    }
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
        let Some(profile) = self.account.as_ref() else {
            return;
        };

        let user = profile.public_key();

        let metadata = Filter::new()
            .kinds(vec![
                Kind::Metadata,
                Kind::ContactList,
                Kind::InboxRelays,
                Kind::MuteList,
                Kind::SimpleGroups,
            ])
            .author(user)
            .limit(10);

        let data = Filter::new().author(user).since(Timestamp::now()).kinds(vec![
            Kind::Metadata,
            Kind::ContactList,
            Kind::MuteList,
            Kind::SimpleGroups,
            Kind::InboxRelays,
            Kind::RelayList,
        ]);

        let msg = Filter::new().kind(Kind::GiftWrap).pubkey(user);
        let new_msg = Filter::new().kind(Kind::GiftWrap).pubkey(user).limit(0);
        let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
        let new_messages_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = &shared_state().client;
            let opts = shared_state().auto_close;

            client.subscribe(metadata, shared_state().auto_close).await?;
            client.subscribe(data, None).await?;
            client.subscribe_with_id(all_messages_sub_id, msg, opts).await?;
            client.subscribe_with_id(new_messages_sub_id, new_msg, None).await?;

            Ok(())
        });

        cx.spawn(async move |_, _| {
            if let Err(e) = task.await {
                log::error!("Error: {}", e);
            }
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
                        this.account = Some(profile);
                        cx.notify();
                        cx.defer_in(window, |this, _, cx| {
                            this.subscribe(cx);
                        });
                    })
                    .ok();
                })
                .ok();
            }
            Err(e) => {
                cx.update(|window, cx| window.push_notification(Notification::error(e.to_string()), cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Create a new account with the given metadata.
    pub fn new_account(&mut self, metadata: Metadata, window: &mut Window, cx: &mut Context<Self>) {
        const DEFAULT_NIP_65_RELAYS: [&str; 4] = [
            "wss://relay.damus.io",
            "wss://relay.primal.net",
            "wss://relay.nostr.net",
            "wss://nos.lol",
        ];

        const DEFAULT_MESSAGING_RELAYS: [&str; 2] = ["wss://auth.nostr1.com", "wss://relay.0xchat.com"];

        let keys = Keys::generate();
        let public_key = keys.public_key();

        let task: Task<Result<Profile, Error>> = cx.background_spawn(async move {
            // Update signer
            shared_state().client.set_signer(keys).await;

            // Set metadata
            shared_state().client.set_metadata(&metadata).await?;

            // Create relay list
            let tags: Vec<Tag> = DEFAULT_NIP_65_RELAYS
                .into_iter()
                .filter_map(|url| {
                    if let Ok(url) = RelayUrl::parse(url) {
                        Some(Tag::relay_metadata(url, None))
                    } else {
                        None
                    }
                })
                .collect();

            let builder = EventBuilder::new(Kind::RelayList, "").tags(tags);

            if let Err(e) = shared_state().client.send_event_builder(builder).await {
                log::error!("Failed to send relay list event: {}", e);
            };

            // Create messaging relay list
            let tags: Vec<Tag> = DEFAULT_MESSAGING_RELAYS
                .into_iter()
                .filter_map(|url| {
                    if let Ok(url) = RelayUrl::parse(url) {
                        Some(Tag::relay(url))
                    } else {
                        None
                    }
                })
                .collect();

            let builder = EventBuilder::new(Kind::InboxRelays, "").tags(tags);

            if let Err(e) = shared_state().client.send_event_builder(builder).await {
                log::error!("Failed to send messaging relay list event: {}", e);
            };

            Ok(Profile::new(public_key, metadata))
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(profile) = task.await {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.account = Some(profile);
                        cx.notify();
                        cx.defer_in(window, |this, _, cx| {
                            this.subscribe(cx);
                        });
                    })
                    .ok();
                })
                .ok();
            } else {
                cx.update(|window, cx| window.push_notification(Notification::error("Failed to create account."), cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Get the reference to account's profile.
    pub fn account(&self) -> Option<&Profile> {
        self.account.as_ref()
    }

    /// Get the reference to the client keys.
    pub fn client_keys(&self) -> Option<&Keys> {
        self.client_keys.as_ref()
    }
}
