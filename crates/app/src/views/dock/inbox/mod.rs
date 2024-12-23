use coop_ui::{
    skeleton::Skeleton, theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt,
};
use gpui::*;
use item::InboxItem;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::cmp::Reverse;

use crate::{get_client, states::chat::ChatRegistry};

pub mod item;

pub struct Inbox {
    label: SharedString,
    items: Model<Option<Vec<View<InboxItem>>>>,
    is_loading: bool,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let items = cx.new_model(|_| None);

        cx.observe_global::<ChatRegistry>(|this, cx| {
            let state = cx.global::<ChatRegistry>();

            if state.reload || (state.is_initialized && state.new_messages.is_empty()) {
                this.load(cx);
            } else {
                #[allow(clippy::collapsible_if)]
                if let Some(items) = this.items.read(cx).as_ref() {
                    // Get all new messages
                    let new_messages = state.new_messages.clone();

                    // Get all current chats
                    let current: Vec<PublicKey> = items
                        .iter()
                        .map(|item| item.model.read(cx).sender)
                        .collect();

                    // Create view for only new chats
                    let new = new_messages
                        .into_iter()
                        .filter(|m| current.iter().any(|pk| pk == &m.event.pubkey))
                        .map(|m| cx.new_view(|cx| InboxItem::new(m.event, cx)))
                        .collect::<Vec<_>>();

                    cx.update_model(&this.items, |a, b| {
                        if let Some(items) = a {
                            items.extend(new);
                            b.notify();
                        }
                    });
                }
            }
        })
        .detach();

        cx.observe_new_views::<InboxItem>(|chat, cx| {
            chat.load_metadata(cx);
        })
        .detach();

        Self {
            items,
            label: "Inbox".into(),
            is_loading: true,
            is_collapsed: false,
        }
    }

    pub fn load(&mut self, cx: &mut ViewContext<Self>) {
        // Hide loading indicator
        self.set_loading(cx);

        let async_items = self.items.clone();
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
                                .sorted_by_key(|ev| Reverse(ev.created_at))
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        }
                    })
                    .await;

                let views: Vec<View<InboxItem>> = events
                    .into_iter()
                    .map(|ev| async_cx.new_view(|cx| InboxItem::new(ev, cx)).unwrap())
                    .collect();

                async_cx.update_model(&async_items, |a, b| {
                    *a = Some(views);
                    b.notify();
                })
            })
            .detach();
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
        } else if let Some(items) = self.items.read(cx).as_ref() {
            content = content.children(items.clone())
        } else {
            // TODO: handle error
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
