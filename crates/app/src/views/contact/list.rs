use std::time::Duration;

use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;

use crate::get_client;

pub struct ContactList {
    contacts: Model<Option<Vec<Contact>>>,
}

impl ContactList {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let contacts = cx.new_model(|_| None);
        let async_contacts = contacts.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();

                async move {
                    if let Ok(contacts) = async_cx
                        .background_executor()
                        .spawn(async move { client.get_contact_list(Duration::from_secs(3)).await })
                        .await
                    {
                        _ = async_cx.update_model(&async_contacts, |model, cx| {
                            *model = Some(contacts);
                            cx.notify();
                        });
                    }
                }
            })
            .detach();

        Self { contacts }
    }
}

impl Render for ContactList {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div().when_some(self.contacts.read(cx).as_ref(), |this, contacts| {
            this.child("Total").child(contacts.len().to_string())
        })
    }
}
