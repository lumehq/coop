use crate::{
    constants::IMAGE_SERVICE,
    get_client,
    states::chat::ChatRegistry,
    states::chat::Room,
    utils::get_room_id,
    utils::{ago, show_npub},
    views::app::{AddPanel, PanelKind},
};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, percentage, Context, InteractiveElement, IntoElement, Model, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, View, ViewContext, VisualContext,
    WindowContext,
};
use nostr_sdk::prelude::*;
use std::sync::Arc;
use ui::{skeleton::Skeleton, theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt};

struct InboxListItem {
    id: SharedString,
    event: Event,
    metadata: Option<Metadata>,
}

impl InboxListItem {
    pub fn new(event: Event, metadata: Option<Metadata>, _cx: &mut ViewContext<'_, Self>) -> Self {
        let id = SharedString::from(get_room_id(&event.pubkey, &event.tags));

        Self {
            id,
            event,
            metadata,
        }
    }

    pub fn action(&self, cx: &mut WindowContext<'_>) {
        let room = Arc::new(Room::parse(&self.event, self.metadata.clone()));

        cx.dispatch_action(Box::new(AddPanel {
            panel: PanelKind::Room(room),
            position: ui::dock::DockPlacement::Center,
        }))
    }
}

impl Render for InboxListItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let ago = ago(self.event.created_at.as_u64());
        let fallback_name = show_npub(self.event.pubkey, 16);

        let mut content = div()
            .font_medium()
            .text_color(cx.theme().sidebar_accent_foreground);

        if let Some(metadata) = self.metadata.clone() {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .map(|this| {
                    if let Some(picture) = metadata.picture.clone() {
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
                    if let Some(display_name) = metadata.display_name.clone() {
                        this.child(display_name)
                    } else {
                        this.child(fallback_name)
                    }
                })
        } else {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .child(
                    img("brand/avatar.png")
                        .flex_shrink_0()
                        .size_6()
                        .rounded_full(),
                )
                .child(fallback_name)
        }

        div()
            .id(self.id.clone())
            .h_8()
            .px_1()
            .flex()
            .items_center()
            .justify_between()
            .text_xs()
            .rounded_md()
            .hover(|this| {
                this.bg(cx.theme().sidebar_accent)
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
            .child(content)
            .child(
                div()
                    .child(ago)
                    .text_color(cx.theme().sidebar_accent_foreground.opacity(0.7)),
            )
            .on_click(cx.listener(|this, _, cx| {
                this.action(cx);
            }))
    }
}

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
            if cx.global::<ChatRegistry>().is_initialized {
                this.load(cx)
            }
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

        // Get all room's events
        let events: Vec<Event> = cx.global::<ChatRegistry>().rooms.read(cx).clone();

        cx.spawn(|view, mut async_cx| async move {
            let client = get_client();
            let mut views = Vec::new();

            for event in events.into_iter() {
                let metadata = async_cx
                    .background_executor()
                    .spawn(async move { client.database().metadata(event.pubkey).await })
                    .await;

                let item = async_cx
                    .new_view(|cx| {
                        if let Ok(metadata) = metadata {
                            InboxListItem::new(event, metadata, cx)
                        } else {
                            InboxListItem::new(event, None, cx)
                        }
                    })
                    .unwrap();

                views.push(item);
            }

            _ = view.update(&mut async_cx, |this, cx| {
                this.items.update(cx, |model, cx| {
                    *model = Some(views);
                    cx.notify()
                });
            });
        })
        .detach();
    }

    fn set_loading(&mut self, cx: &mut ViewContext<Self>) {
        self.is_loading = false;
        cx.notify();
    }
}

impl Collapsible for Inbox {
    fn collapsed(mut self, collapsed: bool) -> Self {
        self.is_collapsed = collapsed;
        self
    }

    fn is_collapsed(&self) -> bool {
        self.is_collapsed
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
