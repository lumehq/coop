use chat::Chat;
use coop_ui::{
    skeleton::Skeleton, theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt,
};
use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::cmp::Reverse;

use crate::{get_client, states::chat::ChatRegistry};

pub mod chat;

pub struct Inbox {
    label: SharedString,
    events: Model<Option<Vec<Event>>>,
    chats: Model<Vec<View<Chat>>>,
    is_loading: bool,
    is_fetching: bool,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let chats = cx.new_model(|_| Vec::new());
        let events = cx.new_model(|_| None);

        cx.observe_global::<ChatRegistry>(|inbox, cx| {
            let state = cx.global::<ChatRegistry>();

            if state.reload || (state.is_initialized && state.new_messages.is_empty()) {
                inbox.load(cx);
            } else {
                let new_messages = state.new_messages.clone();

                for message in new_messages.into_iter() {
                    cx.update_model(&inbox.events, |model, b| {
                        if let Some(events) = model {
                            if !events.iter().any(|ev| ev.pubkey == message.pubkey) {
                                events.push(message);
                                b.notify();
                            }
                        }
                    });
                }
            }
        })
        .detach();

        cx.observe(&events, |inbox, model, cx| {
            // Show fetching indicator
            inbox.set_fetching(cx);

            let events: Option<Vec<Event>> = model.read(cx).clone();

            if let Some(events) = events {
                let views = inbox.chats.read(cx);
                let public_keys: Vec<PublicKey> =
                    views.iter().map(|v| v.read(cx).public_key).collect();

                for event in events
                    .into_iter()
                    .sorted_by_key(|ev| Reverse(ev.created_at))
                {
                    if !public_keys.contains(&event.pubkey) {
                        let view = cx.new_view(|cx| Chat::new(event, cx));

                        cx.update_model(&inbox.chats, |a, b| {
                            a.push(view);
                            b.notify();
                        });
                    }
                }

                // Hide fetching indicator
                inbox.set_fetching(cx);
            }
        })
        .detach();

        cx.observe_new_views::<Chat>(|chat, cx| {
            chat.load_metadata(cx);
        })
        .detach();

        Self {
            events,
            chats,
            label: "Inbox".into(),
            is_loading: true,
            is_fetching: false,
            is_collapsed: false,
        }
    }

    pub fn load(&mut self, cx: &mut ViewContext<Self>) {
        // Hide loading indicator
        self.set_loading(cx);

        let async_events = self.events.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();
                let public_key = signer.get_public_key().await.unwrap();

                let filter = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .pubkey(public_key);

                let events = async_cx
                    .background_executor()
                    .spawn(async move {
                        if let Ok(events) = client.database().query(vec![filter]).await {
                            events
                                .into_iter()
                                .filter(|ev| ev.pubkey != public_key) // Filter all messages from current user
                                .unique_by(|ev| ev.pubkey)
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        }
                    })
                    .await;

                async_cx.update_model(&async_events, |a, b| {
                    *a = Some(events);
                    b.notify();
                })
            })
            .detach();
    }

    fn set_loading(&mut self, cx: &mut ViewContext<Self>) {
        self.is_loading = false;
        cx.notify();
    }

    fn set_fetching(&mut self, cx: &mut ViewContext<Self>) {
        self.is_fetching = !self.is_fetching;
        cx.notify();
    }
}

impl Collapsible for Inbox {
    fn is_collapsed(&self) -> bool {
        self.is_collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.is_collapsed = collapsed;
        self
    }
}

impl Render for Inbox {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div();
        let chats = self.chats.read(cx);

        if self.is_loading {
            content = content.children((0..5).map(|_| {
                div()
                    .h_8()
                    .px_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                    .child(Skeleton::new().w_20().h_3().rounded_sm())
            }))
        } else {
            content = content
                .children(chats.clone())
                .when(self.is_fetching, |this| {
                    this.h_8()
                        .px_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                        .child(Skeleton::new().w_20().h_3().rounded_sm())
                });
        }

        v_flex()
            .gap_1()
            .pt_2()
            .px_2()
            .child(
                div()
                    .id("inbox")
                    .h_7()
                    .px_1()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.7))
                    .hover(|this| this.bg(cx.theme().sidebar_accent.opacity(0.7)))
                    .on_click(cx.listener(move |view, _event, cx| {
                        view.is_collapsed = !view.is_collapsed;
                        cx.notify();
                    }))
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size_6()
                            .when(self.is_collapsed, |this| {
                                this.rotate(percentage(270. / 360.))
                            }),
                    )
                    .child(self.label.clone()),
            )
            .when(!self.is_collapsed, |this| this.child(content))
    }
}
