use chat::Chat;
use coop_ui::{theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt};
use gpui::*;
use itertools::Itertools;
use prelude::FluentBuilder;
use std::cmp::Reverse;

use crate::states::{account::AccountRegistry, chat::ChatRegistry};

pub mod chat;

pub struct Inbox {
    label: SharedString,
    chats: Model<Option<Vec<View<Chat>>>>,
    is_loading: bool,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let chats = cx.new_model(|_| None);

        cx.observe_global::<ChatRegistry>(|inbox, cx| {
            inbox.load(cx);
        })
        .detach();

        Self {
            chats,
            label: "Inbox".into(),
            is_loading: true,
            is_collapsed: false,
        }
    }

    fn load(&mut self, cx: &mut ViewContext<Self>) {
        // Stop loading indicator;
        self.set_loading(cx);

        // Read global chat registry
        let events = cx.global::<ChatRegistry>().get(cx);
        let current_user = cx.global::<AccountRegistry>().get();

        if let Some(public_key) = current_user {
            if let Some(events) = events {
                let chats: Vec<View<Chat>> = events
                    .into_iter()
                    .filter(|ev| ev.pubkey != public_key)
                    .sorted_by_key(|ev| Reverse(ev.created_at))
                    .map(|ev| cx.new_view(|cx| Chat::new(ev, cx)))
                    .collect();

                cx.update_model(&self.chats, |a, b| {
                    *a = Some(chats);
                    b.notify();
                });
            }
        }
    }

    fn set_loading(&mut self, cx: &mut ViewContext<Self>) {
        self.is_loading = false;
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

        if let Some(chats) = self.chats.read(cx).as_ref() {
            content = content.children(chats.clone())
        } else {
            match self.is_loading {
                true => content = content.child("Loading..."),
                false => content = content.child("Empty"),
            }
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
