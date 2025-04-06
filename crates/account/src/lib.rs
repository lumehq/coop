use std::time::Duration;

use anyhow::Error;
use common::profile::NostrProfile;
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
    pub profile: Option<NostrProfile>,
}

impl Account {
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAccount>().0.clone()
    }

    pub fn set_global(account: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAccount(account));
    }

    pub fn login<S>(&mut self, signer: S, window: &mut Window, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        let task: Task<Result<NostrProfile, Error>> = cx.background_spawn(async move {
            let client = get_client();
            // Use user's signer for main signer
            _ = client.set_signer(signer).await;

            // Verify nostr signer and get public key
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Fetch user's metadata
            let metadata = client
                .fetch_metadata(public_key, Duration::from_secs(2))
                .await?
                .unwrap_or_default();

            Ok(NostrProfile::new(public_key, metadata))
        });

        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(profile) => {
                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        this.profile = Some(profile);
                        this.subscribe(cx);
                        cx.notify();
                    })
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

    pub fn new_account(&mut self, metadata: Metadata, window: &mut Window, cx: &mut Context<Self>) {
        let client = get_client();
        let keys = Keys::generate();

        let task: Task<Result<NostrProfile, Error>> = cx.background_spawn(async move {
            let public_key = keys.public_key();
            // Update signer
            client.set_signer(keys).await;
            // Set metadata
            client.set_metadata(&metadata).await?;

            Ok(NostrProfile::new(public_key, metadata))
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(profile) = task.await {
                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        this.profile = Some(profile);
                        this.subscribe(cx);
                        cx.notify();
                    })
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

    pub fn subscribe(&self, cx: &Context<Self>) {
        let Some(profile) = self.profile.as_ref() else {
            return;
        };

        let client = get_client();
        let user = profile.public_key;
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
