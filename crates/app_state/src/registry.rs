use common::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    profile::NostrProfile,
};
use gpui::{AnyView, App, AppContext, Global, WeakEntity};
use nostr_sdk::prelude::*;
use state::get_client;
use std::time::Duration;
use ui::Root;

pub struct AppRegistry {
    root: WeakEntity<Root>,
    user: Option<NostrProfile>,
}

impl Global for AppRegistry {}

impl AppRegistry {
    pub fn set_global(root: WeakEntity<Root>, cx: &mut App) {
        cx.observe_global::<Self>(|cx| {
            if let Some(profile) = cx.global::<Self>().user() {
                let client = get_client();
                let public_key = profile.public_key();

                cx.background_spawn(async move {
                    let subscription = Filter::new()
                        .kind(Kind::ContactList)
                        .author(public_key)
                        .limit(1);

                    // Get contact list
                    _ = client.sync(subscription, &SyncOptions::default()).await;

                    let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
                    let new_message_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

                    // Create a filter for getting all gift wrapped events send to current user
                    let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

                    // Create a filter for getting new message
                    let new_message = Filter::new()
                        .kind(Kind::GiftWrap)
                        .pubkey(public_key)
                        .limit(0);

                    // Subscribe for all messages
                    _ = client
                        .subscribe_with_id(
                            all_messages_sub_id,
                            all_messages,
                            Some(SubscribeAutoCloseOptions::default().exit_policy(
                                ReqExitPolicy::WaitDurationAfterEOSE(Duration::from_secs(5)),
                            )),
                        )
                        .await;

                    // Subscribe for new message
                    _ = client
                        .subscribe_with_id(new_message_sub_id, new_message, None)
                        .await;
                })
                .detach();
            }
        })
        .detach();

        cx.set_global(Self { root, user: None });
    }

    pub fn set_user(&mut self, profile: Option<NostrProfile>) {
        self.user = profile;
    }

    pub fn user(&self) -> Option<NostrProfile> {
        self.user.clone()
    }

    pub fn set_root_view(&self, view: AnyView, cx: &mut App) {
        if let Err(e) = self.root.update(cx, |this, cx| {
            this.set_view(view, cx);
        }) {
            println!("Error: {}", e)
        }
    }
}
