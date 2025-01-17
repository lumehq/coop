use crate::{get_client, states::chat::room::Member};
use gpui::{
    div, img, impl_internal_actions, px, uniform_list, Context, FocusHandle, InteractiveElement,
    IntoElement, Model, ParentElement, Render, StatefulInteractiveElement, Styled, View,
    ViewContext, VisualContext, WindowContext,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use std::collections::HashSet;
use ui::{
    indicator::Indicator,
    input::TextInput,
    prelude::FluentBuilder,
    theme::{scale::ColorScaleStep, ActiveTheme},
    Icon, IconName, Sizable, StyledExt,
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_internal_actions!(contacts, [SelectContact]);

pub struct Compose {
    input: View<TextInput>,
    contacts: Model<Option<Vec<Member>>>,
    selected: Model<HashSet<PublicKey>>,
    focus_handle: FocusHandle,
}

impl Compose {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let contacts = cx.new_model(|_| None);
        let selected = cx.new_model(|_| HashSet::new());
        let input = cx.new_view(|cx| {
            TextInput::new(cx)
                .appearance(false)
                .text_size(ui::Size::Small)
                .placeholder("npub1...")
                .cleanable()
        });

        cx.spawn(|this, mut async_cx| {
            let client = get_client();

            async move {
                let query: anyhow::Result<Vec<Member>, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn(async move {
                        let signer = client.signer().await?;
                        let public_key = signer.get_public_key().await?;
                        let profiles = client.database().contacts(public_key).await?;
                        let members: Vec<Member> = profiles
                            .into_iter()
                            .map(|profile| Member::new(profile.public_key(), profile.metadata()))
                            .collect();

                        Ok(members)
                    })
                    .await;

                if let Ok(contacts) = query {
                    if let Some(view) = this.upgrade() {
                        _ = async_cx.update_view(&view, |this, cx| {
                            this.contacts.update(cx, |this, cx| {
                                *this = Some(contacts);
                                cx.notify();
                            });

                            cx.notify();
                        });
                    }
                }
            }
        })
        .detach();

        Self {
            input,
            contacts,
            selected,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn selected<'a>(&self, cx: &'a WindowContext) -> Vec<&'a PublicKey> {
        self.selected.read(cx).iter().collect()
    }

    fn on_action_select(&mut self, action: &SelectContact, cx: &mut ViewContext<Self>) {
        self.selected.update(cx, |this, cx| {
            if this.contains(&action.0) {
                this.remove(&action.0);
            } else {
                this.insert(action.0);
            };
            cx.notify();
        });

        // TODO
    }
}

impl Render for Compose {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let msg =
            "Start a conversation with someone using their npub or NIP-05 (like foo@bar.com).";

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_select))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                            .child(msg),
                    )
                    .child(
                        div()
                            .bg(cx.theme().base.step(cx, ColorScaleStep::FOUR))
                            .rounded(px(cx.theme().radius))
                            .px_2()
                            .child(self.input.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Contacts"))
                    .child(div().map(|this| {
                        if let Some(contacts) = self.contacts.read(cx).clone() {
                            this.child(
                                uniform_list(
                                    cx.view().clone(),
                                    "contacts",
                                    contacts.len(),
                                    move |this, range, cx| {
                                        let selected = this.selected.read(cx);
                                        let mut items = Vec::new();

                                        for ix in range {
                                            let item = contacts.get(ix).unwrap().clone();
                                            let is_select = selected.contains(&item.public_key());

                                            items.push(
                                                div()
                                                    .id(ix)
                                                    .w_full()
                                                    .h_10()
                                                    .px_1p5()
                                                    .rounded(px(cx.theme().radius))
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .text_sm()
                                                            .child(
                                                                div().flex_shrink_0().child(
                                                                    img(item.avatar()).size_8(),
                                                                ),
                                                            )
                                                            .child(item.name()),
                                                    )
                                                    .when(is_select, |this| {
                                                        this.child(
                                                            Icon::new(IconName::CircleCheck)
                                                                .size_4()
                                                                .text_color(cx.theme().base.step(
                                                                    cx,
                                                                    ColorScaleStep::TWELVE,
                                                                )),
                                                        )
                                                    })
                                                    .hover(|this| {
                                                        this.bg(cx
                                                            .theme()
                                                            .base
                                                            .step(cx, ColorScaleStep::FOUR))
                                                            .text_color(
                                                                cx.theme().base.step(
                                                                    cx,
                                                                    ColorScaleStep::ELEVEN,
                                                                ),
                                                            )
                                                    })
                                                    .on_click(move |_, cx| {
                                                        cx.dispatch_action(Box::new(
                                                            SelectContact(item.public_key()),
                                                        ));
                                                    }),
                                            );
                                        }

                                        items
                                    },
                                )
                                .h(px(320.)),
                            )
                        } else {
                            this.flex()
                                .items_center()
                                .justify_center()
                                .h_16()
                                .child(Indicator::new().small())
                        }
                    })),
            )
    }
}
