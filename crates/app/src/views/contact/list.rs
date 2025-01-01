use gpui::*;
use prelude::FluentBuilder;
use std::time::Duration;

use super::item::ContactListItem;
use crate::get_client;

pub struct ContactList {
    contacts: Model<Option<Vec<View<ContactListItem>>>>,
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
                        let views: Vec<View<ContactListItem>> = contacts
                            .into_iter()
                            .map(|contact| {
                                async_cx
                                    .new_view(|cx| ContactListItem::new(contact.public_key, cx))
                                    .unwrap()
                            })
                            .collect();

                        _ = async_cx.update_model(&async_contacts, |model, cx| {
                            *model = Some(views);
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
            this.children(contacts.clone())
        })
    }
}
