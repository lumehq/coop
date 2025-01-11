use crate::{constants::IMAGE_SERVICE, states::chat::ChatRegistry, utils::ago};
use gpui::{
    div, img, percentage, prelude::FluentBuilder, InteractiveElement, IntoElement, ParentElement,
    Render, RenderOnce, SharedString, StatefulInteractiveElement, Styled, ViewContext,
    WindowContext,
};
use ui::{skeleton::Skeleton, theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt};

pub struct Inbox {
    label: SharedString,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(_cx: &mut ViewContext<'_, Self>) -> Self {
        Self {
            label: "Inbox".into(),
            is_collapsed: false,
        }
    }

    fn skeleton(&self, total: i32) -> impl IntoIterator<Item = impl IntoElement> {
        (0..total).map(|_| {
            div()
                .h_8()
                .px_1()
                .flex()
                .items_center()
                .gap_2()
                .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                .child(Skeleton::new().w_20().h_3().rounded_sm())
        })
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
        let weak_model = cx.global::<ChatRegistry>().inbox();

        if let Some(model) = weak_model.upgrade() {
            content = content.children(model.read(cx).iter().map(|model| {
                let room = model.read(cx);
                let id = room.id.to_string().into();
                let ago = ago(room.last_seen.as_u64()).into();
                // Get first member
                let sender = room.members.first().unwrap();
                // Compute group name based on member' names
                let name: SharedString = room
                    .members
                    .iter()
                    .map(|profile| profile.name())
                    .collect::<Vec<String>>()
                    .join(", ")
                    .into();

                InboxListItem::new(
                    id,
                    ago,
                    room.is_group,
                    name,
                    sender.metadata().picture,
                    sender.name(),
                )
            }))
        } else {
            content = content.children(self.skeleton(5))
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

#[derive(Clone, IntoElement)]
struct InboxListItem {
    id: SharedString,
    ago: SharedString,
    is_group: bool,
    group_name: SharedString,
    sender_avatar: Option<String>,
    sender_name: String,
}

impl InboxListItem {
    pub fn new(
        id: SharedString,
        ago: SharedString,
        is_group: bool,
        group_name: SharedString,
        sender_avatar: Option<String>,
        sender_name: String,
    ) -> Self {
        Self {
            id,
            ago,
            is_group,
            group_name,
            sender_avatar,
            sender_name,
        }
    }
}

impl RenderOnce for InboxListItem {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let mut content = div()
            .font_medium()
            .text_color(cx.theme().sidebar_accent_foreground);

        if self.is_group {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .child(img("brand/avatar.png").size_6().rounded_full())
                .child(self.group_name)
        } else {
            content = content.flex().items_center().gap_2().map(|mut this| {
                // Avatar
                if let Some(picture) = self.sender_avatar {
                    this = this.child(
                        img(format!(
                            "{}/?url={}&w=72&h=72&fit=cover&mask=circle&n=-1",
                            IMAGE_SERVICE, picture
                        ))
                        .flex_shrink_0()
                        .size_6()
                        .rounded_full(),
                    );
                } else {
                    this = this.child(
                        img("brand/avatar.png")
                            .flex_shrink_0()
                            .size_6()
                            .rounded_full(),
                    );
                }

                // Display name
                this = this.child(self.sender_name);

                this
            })
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
                    .child(self.ago)
                    .text_color(cx.theme().sidebar_accent_foreground.opacity(0.7)),
            )
    }
}
