use async_utility::task::spawn;
use gpui::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    get_client,
};

pub struct AccountRegistry {
    public_key: Option<PublicKey>,
}

impl Global for AccountRegistry {}

impl AccountRegistry {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());

        cx.observe_global::<Self>(|cx| {
            let state = cx.global::<Self>();

            if let Some(public_key) = state.public_key {
                let client = get_client();

                let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
                let new_message_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

                // Create a filter for getting all gift wrapped events send to current user
                let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

                // Subscription options
                let opts = SubscribeAutoCloseOptions::default()
                    .filter(FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(5)));

                // Create a filter for getting new message
                let new_message = Filter::new()
                    .kind(Kind::GiftWrap)
                    .pubkey(public_key)
                    .limit(0);

                spawn(async move {
                    // Subscribe for all messages
                    if client
                        .subscribe_with_id(all_messages_sub_id, vec![all_messages], Some(opts))
                        .await
                        .is_ok()
                    {
                        // Subscribe for new message
                        _ = client
                            .subscribe_with_id(new_message_sub_id, vec![new_message], None)
                            .await
                    }
                });
            }
        })
        .detach();
    }

    pub fn set_user(&mut self, public_key: Option<PublicKey>) {
        self.public_key = public_key
    }

    pub fn is_user_logged_in(&self) -> bool {
        self.public_key.is_some()
    }

    fn new() -> Self {
        Self { public_key: None }
    }
}
