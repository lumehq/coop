use crate::{
    constants::IMAGE_SERVICE, get_client, states::account::AccountRegistry, utils::show_npub,
};
use gpui::{
    div, img, impl_actions, list, px, Context, ElementId, FocusHandle, InteractiveElement,
    IntoElement, ListAlignment, ListState, Model, ParentElement, Pixels, Render, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, ViewContext, WindowContext,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeSet, HashSet};
use ui::{
    prelude::FluentBuilder,
    theme::{ActiveTheme, Colorize},
    Icon, IconName, Selectable, StyledExt,
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_actions!(contacts, [SelectContact]);

#[derive(Clone, IntoElement)]
struct ContactListItem {
    id: ElementId,
    public_key: PublicKey,
    metadata: Option<Metadata>,
    selected: bool,
}

impl ContactListItem {
    pub fn new(public_key: PublicKey, metadata: Option<Metadata>) -> Self {
        let id = SharedString::from(public_key.to_hex()).into();

        Self {
            id,
            public_key,
            metadata,
            selected: false,
        }
    }
}

impl Selectable for ContactListItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn element_id(&self) -> &gpui::ElementId {
        &self.id
    }
}

impl RenderOnce for ContactListItem {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let fallback = show_npub(self.public_key, 16);
        let mut content = div().flex().items_center().gap_2().text_sm();

        if let Some(metadata) = self.metadata {
            content = content
                .map(|this| {
                    if let Some(picture) = metadata.picture {
                        this.flex_shrink_0().child(
                            img(format!(
                                "{}/?url={}&w=72&h=72&fit=cover&mask=circle&n=-1",
                                IMAGE_SERVICE, picture
                            ))
                            .size_6(),
                        )
                    } else {
                        this.flex_shrink_0()
                            .child(img("brand/avatar.png").size_6().rounded_full())
                    }
                })
                .map(|this| {
                    if let Some(display_name) = metadata.display_name {
                        this.flex_1().child(display_name)
                    } else {
                        this.flex_1().child(fallback)
                    }
                })
        } else {
            content = content
                .child(img("brand/avatar.png").size_6().rounded_full())
                .child(fallback)
        }

        div()
            .id(self.id)
            .w_full()
            .h_8()
            .px_1()
            .rounded_md()
            .flex()
            .items_center()
            .justify_between()
            .child(content)
            .when(self.selected, |this| {
                this.child(
                    Icon::new(IconName::CircleCheck)
                        .size_4()
                        .text_color(cx.theme().primary),
                )
            })
            .hover(|this| {
                this.bg(cx.theme().muted.darken(0.1))
                    .text_color(cx.theme().muted_foreground.darken(0.1))
            })
            .on_click(move |_, cx| {
                cx.dispatch_action(Box::new(SelectContact(self.public_key)));
            })
    }
}

#[derive(Clone)]
struct Contacts {
    #[allow(dead_code)]
    count: usize,
    items: Vec<ContactListItem>,
}

pub struct ContactList {
    list: ListState,
    contacts: Model<BTreeSet<Profile>>,
    selected: HashSet<PublicKey>,
    focus_handle: FocusHandle,
}

impl ContactList {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let list = ListState::new(0, ListAlignment::Top, Pixels(50.), move |_, _| {
            div().into_any_element()
        });

        let contacts = cx.new_model(|_| BTreeSet::new());
        let async_contacts = contacts.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();
                let current_user = cx.global::<AccountRegistry>().get();

                async move {
                    if let Some(public_key) = current_user {
                        if let Ok(profiles) = async_cx
                            .background_executor()
                            .spawn(async move { client.database().contacts(public_key).await })
                            .await
                        {
                            _ = async_cx.update_model(&async_contacts, |model, cx| {
                                *model = profiles;
                                cx.notify();
                            });
                        }
                    }
                }
            })
            .detach();

        cx.observe(&contacts, |this, model, cx| {
            let profiles = model.read(cx).clone();
            let contacts = Contacts {
                count: profiles.len(),
                items: profiles
                    .into_iter()
                    .map(|contact| {
                        ContactListItem::new(contact.public_key(), Some(contact.metadata()))
                    })
                    .collect(),
            };

            this.list = ListState::new(
                contacts.items.len(),
                ListAlignment::Top,
                Pixels(50.),
                move |idx, _cx| {
                    let item = contacts.items.get(idx).unwrap().clone();
                    div().child(item).into_any_element()
                },
            );

            cx.notify();
        })
        .detach();

        Self {
            list,
            contacts,
            selected: HashSet::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn selected(&self) -> Vec<PublicKey> {
        self.selected.clone().into_iter().collect()
    }

    fn on_action_select(&mut self, action: &SelectContact, cx: &mut ViewContext<Self>) {
        self.selected.insert(action.0);

        let profiles = self.contacts.read(cx).clone();
        let contacts = Contacts {
            count: profiles.len(),
            items: profiles
                .into_iter()
                .map(|contact| {
                    let public_key = contact.public_key();
                    let metadata = contact.metadata();

                    ContactListItem::new(public_key, Some(metadata))
                        .selected(self.selected.contains(&public_key))
                })
                .collect(),
        };

        self.list = ListState::new(
            contacts.items.len(),
            ListAlignment::Top,
            Pixels(50.),
            move |idx, _cx| {
                let item = contacts.items.get(idx).unwrap().clone();
                div().child(item).into_any_element()
            },
        );

        cx.notify();
    }
}

impl Render for ContactList {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_select))
            .flex()
            .flex_col()
            .gap_1()
            .child(div().font_semibold().child("Contacts"))
            .child(
                div()
                    .p_1()
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
                    .rounded_lg()
                    .child(list(self.list.clone()).h(px(300.))),
            )
    }
}