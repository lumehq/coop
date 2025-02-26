use anyhow::{anyhow, Error};
use common::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    profile::NostrProfile,
};
use gpui::{App, AppContext, AsyncApp, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use state::get_client;
use std::{sync::Arc, time::Duration};

pub fn login(signer: Arc<dyn NostrSigner>, cx: &AsyncApp) -> Task<Result<(), Error>> {
    let client = get_client();
    let task: Task<Result<NostrProfile, Error>> = cx.background_spawn(async move {
        // Update nostr signer
        _ = client.set_signer(signer).await;

        // Verify nostr signer and get public key
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;
        let metadata = client
            .fetch_metadata(public_key, Duration::from_secs(2))
            .await
            .unwrap_or_default();

        Ok(NostrProfile::new(public_key, metadata))
    });

    cx.spawn(|cx| async move {
        match task.await {
            Ok(profile) => {
                cx.update(|cx| {
                    let this = cx.new(|cx| {
                        let this = Account { profile };
                        // Run initial setup for this account
                        this.setup(cx);
                        this
                    });

                    Account::set_global(this, cx)
                })
            }
            Err(e) => Err(anyhow!("Login failed: {}", e)),
        }
    })
}

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

    /// Get the account's profile
    pub fn profile(&self) -> &NostrProfile {
        &self.profile
    }

    /// Create a task to verify inbox relays
    pub fn verify_inbox_relays(&self, cx: &App) -> Task<Result<Vec<String>, Error>> {
        let client = get_client();
        let public_key = self.profile.public_key();

        cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            // Get inbox relays event from database
            let events = client.database().query(filter).await?;

            if let Some(event) = events.first_owned() {
                let relays = event
                    .tags
                    .filter_standardized(TagKind::Relay)
                    .filter_map(|t| {
                        if let TagStandard::Relay(url) = t {
                            Some(url.to_string())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                Ok(relays)
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    /// Run initial setup for logging in account
    fn setup(&self, cx: &mut Context<Self>) {
        let client = get_client();
        let public_key = self.profile.public_key();

        cx.background_spawn(async move {
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

            // Get contact list
            let contact_list = Filter::new()
                .kind(Kind::ContactList)
                .author(public_key)
                .limit(1);

            if let Err(e) = client.subscribe(contact_list, Some(opts)).await {
                log::error!("Failed to get contact list: {}", e);
            }

            // Create a filter to continuously receive new user's data.
            let data = Filter::new()
                .kinds(vec![Kind::Metadata, Kind::InboxRelays, Kind::RelayList])
                .author(public_key)
                .since(Timestamp::now());

            if let Err(e) = client.subscribe(data, None).await {
                log::error!("Failed to subscribe to user data: {}", e);
            }

            // Create a filter for getting all gift wrapped events send to current user
            let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);

            if let Err(e) = client
                .subscribe_with_id(sub_id, filter.clone(), Some(opts))
                .await
            {
                log::error!("Failed to subscribe to all messages: {}", e);
            }

            // Create a filter to continuously receive new messages.
            let new_filter = filter.limit(0);
            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

            if let Err(e) = client.subscribe_with_id(sub_id, new_filter, None).await {
                log::error!("Failed to subscribe to new messages: {}", e);
            }
        })
        .detach();
    }
}
