use anyhow::anyhow;
use common::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    profile::NostrProfile,
};
use gpui::{App, AppContext, AsyncApp, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use state::get_client;
use std::{sync::Arc, time::Duration};

struct GlobalAccount(Entity<Account>);

impl Global for GlobalAccount {}

#[derive(Debug, Clone)]
pub struct Account {
    profile: NostrProfile,
}

impl Account {
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalAccount>()
            .map(|model| model.0.clone())
    }

    pub fn set_global(account: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAccount(account));
    }

    pub fn login(signer: Arc<dyn NostrSigner>, cx: &AsyncApp) -> Task<Result<(), anyhow::Error>> {
        let client = get_client();
        let (tx, rx) = oneshot::channel::<Option<NostrProfile>>();

        cx.background_spawn(async move {
            // Update nostr signer
            _ = client.set_signer(signer).await;
            // Verify nostr signer and get public key
            let result = async {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let metadata = client
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await
                    .ok()
                    .unwrap_or_default();

                Ok::<_, anyhow::Error>(NostrProfile::new(public_key, metadata))
            }
            .await;

            tx.send(result.ok()).ok();
        })
        .detach();

        cx.spawn(|cx| async move {
            if let Ok(Some(profile)) = rx.await {
                cx.update(|cx| {
                    let this = cx.new(|cx| {
                        let this = Account { profile };
                        // Run initial sync data for this account
                        if let Some(task) = this.sync(cx) {
                            task.detach();
                        }
                        // Return
                        this
                    });

                    Self::set_global(this, cx)
                })
            } else {
                Err(anyhow!("Login failed"))
            }
        })
    }

    pub fn get(&self) -> &NostrProfile {
        &self.profile
    }

    fn sync(&self, cx: &mut Context<Self>) -> Option<Task<()>> {
        let client = get_client();
        let public_key = self.profile.public_key();

        let task = cx.background_spawn(async move {
            // Set the default options for this task
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

            // Create a filter to get contact list
            let contact_list = Filter::new()
                .kind(Kind::ContactList)
                .author(public_key)
                .limit(1);

            if let Err(e) = client.subscribe(contact_list, Some(opts)).await {
                log::error!("Failed to subscribe to contact list: {}", e);
            }

            // Create a filter for getting all gift wrapped events send to current user
            let msg = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
            let id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);

            if let Err(e) = client.subscribe_with_id(id, msg.clone(), Some(opts)).await {
                log::error!("Failed to subscribe to all messages: {}", e);
            }

            // Create a filter to continuously receive new messages.
            let new_msg = msg.limit(0);
            let id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

            if let Err(e) = client.subscribe_with_id(id, new_msg, None).await {
                log::error!("Failed to subscribe to new messages: {}", e);
            }
        });

        Some(task)
    }
}
