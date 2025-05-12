use std::time::Duration;

use anyhow::Error;
use global::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    get_client,
};
use gpui::{App, AppContext, Context, Entity, Global, Task, Window};
use nostr_sdk::prelude::*;
use ui::{notification::Notification, ContextModal};

struct GlobalAccount(Entity<Account>);

impl Global for GlobalAccount {}

pub fn init(cx: &mut App) {
    Account::set_global(cx.new(|_| Account { profile: None }), cx);
}

#[derive(Debug, Clone)]
pub struct Account {
    pub profile: Option<Profile>,
}

impl Account {
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAccount>().0.clone()
    }

    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalAccount>().0.read(cx)
    }

    pub fn set_global(account: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAccount(account));
    }

    /// Login to the account using the given signer.
    pub fn login<S>(&mut self, signer: S, window: &mut Window, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        let task: Task<Result<Profile, Error>> = cx.background_spawn(async move {
            let client = get_client();
            let public_key = signer.get_public_key().await?;

            // Update signer
            client.set_signer(signer).await;

            // Fetch user's metadata
            let metadata = client
                .fetch_metadata(public_key, Duration::from_secs(2))
                .await?
                .unwrap_or_default();

            Ok(Profile::new(public_key, metadata))
        });

        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(profile) => {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.profile(profile, cx);

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
    pub fn new_account(&mut self, metadata: Metadata, window: &mut Window, cx: &mut Context<Self>) {
        const DEFAULT_NIP_65_RELAYS: [&str; 4] = [
            "wss://relay.damus.io",
            "wss://relay.primal.net",
            "wss://relay.nostr.net",
            "wss://nos.lol",
        ];

        const DEFAULT_MESSAGING_RELAYS: [&str; 2] =
            ["wss://auth.nostr1.com", "wss://relay.0xchat.com"];

        let keys = Keys::generate();
        let public_key = keys.public_key();

        let task: Task<Result<Profile, Error>> = cx.background_spawn(async move {
            let client = get_client();

            // Update signer
            client.set_signer(keys).await;

            // Set metadata
            client.set_metadata(&metadata).await?;

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

            if let Err(e) = client.send_event_builder(builder).await {
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

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send messaging relay list event: {}", e);
            };

            Ok(Profile::new(public_key, metadata))
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(profile) = task.await {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.profile(profile, cx);

                        cx.defer_in(window, |this, _, cx| {
                            this.subscribe(cx);
                        });
                    })
                    .ok();
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    window.push_notification(Notification::error("Failed to create account."), cx)
                })
                .ok();
            }
        })
        .detach();
    }

    /// Get the reference to profile.
    pub fn profile_ref(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// Sets the profile for the account.
    pub fn profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        self.profile = Some(profile);
        cx.notify();
    }

    /// Subscribes to the current account's metadata.
    pub fn subscribe(&self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.as_ref() else {
            return;
        };

        let user = profile.public_key();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

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

        let data = Filter::new()
            .author(user)
            .since(Timestamp::now())
            .kinds(vec![
                Kind::Metadata,
                Kind::ContactList,
                Kind::MuteList,
                Kind::SimpleGroups,
                Kind::InboxRelays,
                Kind::RelayList,
            ]);

        let msg = Filter::new().kind(Kind::GiftWrap).pubkey(user);
        let new_msg = Filter::new().kind(Kind::GiftWrap).pubkey(user).limit(0);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = get_client();
            client.subscribe(metadata, Some(opts)).await?;
            client.subscribe(data, None).await?;

            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            client.subscribe_with_id(sub_id, msg, Some(opts)).await?;

            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
            client.subscribe_with_id(sub_id, new_msg, None).await?;

            Ok(())
        });

        cx.spawn(async move |_, _| {
            if let Err(e) = task.await {
                log::error!("Error: {}", e);
            }
        })
        .detach();
    }
}
