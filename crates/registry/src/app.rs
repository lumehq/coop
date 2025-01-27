use common::constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID};
use gpui::{AppContext, Context, Global, Model, WeakModel};
use nostr_sdk::prelude::*;
use state::get_client;
use std::time::Duration;

use crate::contact::Contact;

pub struct AppRegistry {
    user: Entity<Option<Contact>>,
    pub is_loading: bool,
}

impl Global for AppRegistry {}

impl AppRegistry {
    pub fn set_global(cx: &mut AppContext) {
        let user: Entity<Option<Contact>> = cx.new(|_| None);
        let is_loading = true;

        cx.observe(&user, |this, cx| {
            if let Some(contact) = this.read(cx).as_ref() {
                let client = get_client();
                let public_key = contact.public_key();

                cx.background_executor()
                    .spawn(async move {
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

                        let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
                        let new_message_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

                        // Create a filter for getting all gift wrapped events send to current user
                        let all_messages = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

                        // Subscription options
                        let opts = SubscribeAutoCloseOptions::default().exit_policy(
                            ReqExitPolicy::WaitDurationAfterEOSE(Duration::from_secs(5)),
                        );

                        // Create a filter for getting new message
                        let new_message = Filter::new()
                            .kind(Kind::GiftWrap)
                            .pubkey(public_key)
                            .limit(0);

                        // Subscribe for all messages
                        _ = client
                            .subscribe_with_id(all_messages_sub_id, vec![all_messages], Some(opts))
                            .await;

                        // Subscribe for new message
                        _ = client
                            .subscribe_with_id(new_message_sub_id, vec![new_message], None)
                            .await;
                    })
                    .detach();
            }
        })
        .detach();

        cx.set_global(Self { user, is_loading });
    }

    pub fn user(&self) -> WeakEntity<Option<Contact>> {
        self.user.downgrade()
    }

    pub fn current_user(&self, window: &Window, cx: &App) -> Option<Contact> {
        self.user.read(cx).clone()
    }

    pub fn set_user(&mut self, contact: Contact, cx: &mut AppContext) {
        self.user.update(cx, |this, cx| {
            *this = Some(contact);
            cx.notify();
        });

        self.is_loading = false;
    }

    pub fn logout(&mut self, cx: &mut AppContext) {
        self.user.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });
    }
}
