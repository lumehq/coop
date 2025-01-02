use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::cmp::Reverse;
use ui::{skeleton::Skeleton, theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt};

use super::inbox::item::InboxListItem;
use crate::{get_client, states::chat::ChatRegistry, utils::get_room_id};

pub mod item;

pub struct Inbox {
    label: SharedString,
    items: Model<Option<Vec<View<InboxListItem>>>>,
    is_loading: bool,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let items = cx.new_model(|_| None);

        cx.observe_global::<ChatRegistry>(|this, cx| {
            let state = cx.global::<ChatRegistry>();
            let empty_messages = state.new_messages.read().unwrap().is_empty();

            if state.reload || (state.is_initialized && empty_messages) {
                this.load(cx);
            } else {
                #[allow(clippy::collapsible_if)]
                if let Some(items) = this.items.read(cx).as_ref() {
                    // Get all current chats
                    let current_rooms: Vec<String> =
                        items.iter().map(|item| item.model.read(cx).id()).collect();

                    // Get all new messages
                    let messages = state
                        .new_messages
                        .read()
                        .unwrap()
                        .clone()
                        .into_iter()
                        .filter(|m| {
                            let keys = m.event.tags.public_keys().copied().collect::<Vec<_>>();
                            let new_id = get_room_id(&m.event.pubkey, &keys);

                            !current_rooms.iter().any(|id| id == &new_id)
                        })
                        .collect::<Vec<_>>();

                    // Create view for new chats only
                    let new = messages
                        .into_iter()
                        .map(|m| cx.new_view(|cx| InboxListItem::new(m.event, cx)))
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

        cx.observe_new_views::<InboxListItem>(|item, cx| {
            item.request_metadata(cx);
            item.load_metadata(cx);
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

        let items = self.items.read(cx).as_ref();
        // Get all current rooms id
        let current_rooms: Vec<String> = if let Some(items) = items {
            items.iter().map(|item| item.model.read(cx).id()).collect()
        } else {
            Vec::new()
        };

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

                let views: Vec<View<InboxListItem>> = events
                    .into_iter()
                    .filter(|ev| {
                        let keys = ev.tags.public_keys().copied().collect::<Vec<_>>();
                        let new_id = get_room_id(&ev.pubkey, &keys);

                        !current_rooms.iter().any(|id| id == &new_id)
                    })
                    .map(|ev| async_cx.new_view(|cx| InboxListItem::new(ev, cx)).unwrap())
                    .collect();

                async_cx.update_model(&async_items, |model, cx| {
                    if let Some(items) = model {
                        items.extend(views);
                    } else {
                        *model = Some(views);
                    }

                    cx.notify();
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
            .px_2()
            .gap_1()
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
