use async_utility::task::spawn;
use gpui::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::{constants::SUBSCRIPTION_ID, get_client};

pub struct AccountState {
    pub in_use: Option<PublicKey>,
}

impl Global for AccountState {}

impl AccountState {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());

        cx.observe_global::<Self>(|cx| {
            let state = cx.global::<Self>();

            if let Some(public_key) = state.in_use {
                let client = get_client();

                // Create a filter for getting all gift wrapped events send to current user
                let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

                // Subscription options
                let opts = SubscribeAutoCloseOptions::default().filter(
                    FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(10)),
                );

                let subscription_id = SubscriptionId::new(SUBSCRIPTION_ID);

                // Create a filter for getting new message
                let new_message = Filter::new()
                    .kind(Kind::GiftWrap)
                    .pubkey(public_key)
                    .limit(0);

                spawn(async move {
                    if client
                        .subscribe(vec![all_messages], Some(opts))
                        .await
                        .is_ok()
                    {
                        // Subscribe for new message
                        _ = client
                            .subscribe_with_id(subscription_id, vec![new_message], None)
                            .await
                    }
                });
            }
        })
        .detach();
    }

    fn new() -> Self {
        Self { in_use: None }
    }
}
