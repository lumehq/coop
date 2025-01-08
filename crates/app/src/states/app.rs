use crate::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    get_client,
};
use gpui::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

pub struct AppRegistry {
    user: Model<Option<PublicKey>>,
    refreshs: Model<Vec<PublicKey>>,
    pub(crate) is_loading: bool,
}

impl Global for AppRegistry {}

impl AppRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let refreshs = cx.new_model(|_| Vec::new());
        let user = cx.new_model(|_| None);
        let async_user = user.clone();

        cx.set_global(Self {
            user,
            refreshs,
            is_loading: true,
        });

        cx.observe(&async_user, |model, cx| {
            if let Some(public_key) = model.read(cx).clone().as_ref() {
                let client = get_client();
                let public_key = *public_key;

                let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
                let new_message_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

                // Create a filter for getting all gift wrapped events send to current user
                let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

                // Subscription options
                let opts = SubscribeAutoCloseOptions::default()
                    .exit_policy(ReqExitPolicy::WaitDurationAfterEOSE(Duration::from_secs(5)));

                // Create a filter for getting new message
                let new_message = Filter::new()
                    .kind(Kind::GiftWrap)
                    .pubkey(public_key)
                    .limit(0);

                cx.background_executor()
                    .spawn(async move {
                        // Subscribe for all messages
                        _ = client
                            .subscribe_with_id(all_messages_sub_id, vec![all_messages], Some(opts))
                            .await;

                        // Subscribe for new message
                        _ = client
                            .subscribe_with_id(new_message_sub_id, vec![new_message], None)
                            .await;

                        let subscription = Filter::new()
                            .kind(Kind::Metadata)
                            .author(public_key)
                            .limit(1);

                        // Get metadata
                        _ = client.sync(subscription, &SyncOptions::default()).await;

                        let subscription = Filter::new()
                            .kind(Kind::ContactList)
                            .author(public_key)
                            .limit(1);

                        // Get contact list
                        _ = client.sync(subscription, &SyncOptions::default()).await;
                    })
                    .detach();
            }
        })
        .detach();
    }

    pub fn set_loading(&mut self) {
        self.is_loading = false
    }

    pub fn set_user(&mut self, public_key: PublicKey, cx: &mut AppContext) {
        self.user.update(cx, |model, cx| {
            *model = Some(public_key);
            cx.notify();
        });

        self.is_loading = false;
    }

    pub fn set_refresh(&mut self, public_key: PublicKey, cx: &mut AppContext) {
        self.refreshs.update(cx, |this, cx| {
            this.push(public_key);
            cx.notify();
        })
    }

    pub fn current_user(&self) -> WeakModel<Option<PublicKey>> {
        self.user.downgrade()
    }

    pub fn refreshs(&self) -> WeakModel<Vec<PublicKey>> {
        self.refreshs.downgrade()
    }
}
