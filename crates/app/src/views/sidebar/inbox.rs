use crate::{
    states::chat::ChatRegistry,
    utils::ago,
    views::app::{AddPanel, PanelKind},
};
use gpui::{
    div, img, percentage, prelude::FluentBuilder, px, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, ViewContext,
};
use ui::{
    skeleton::Skeleton,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, Collapsible, Icon, IconName, StyledExt,
};

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

    fn render_skeleton(&self, total: i32) -> impl IntoIterator<Item = impl IntoElement> {
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

    fn render_item(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let weak_model = cx.global::<ChatRegistry>().inbox();

        if let Some(model) = weak_model.upgrade() {
            div().map(|this| {
                let inbox = model.read(cx);

                if inbox.is_loading {
                    this.children(self.render_skeleton(5))
                } else {
                    this.children(inbox.rooms.iter().map(|model| {
                        let room = model.read(cx);
                        let id = room.id;
                        let room_id: SharedString = id.to_string().into();
                        let ago: SharedString = ago(room.last_seen).into();

                        div()
                            .id(room_id)
                            .h_8()
                            .px_1()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .rounded(px(cx.theme().radius))
                            .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::FOUR)))
                            .child(div().font_medium().map(|this| {
                                if room.is_group {
                                    this.flex()
                                        .items_center()
                                        .gap_2()
                                        .child(img("brand/avatar.png").size_6().rounded_full())
                                        .child(room.name())
                                } else {
                                    this.when_some(room.members.first(), |this, sender| {
                                        this.flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                img(sender.avatar())
                                                    .size_6()
                                                    .rounded_full()
                                                    .flex_shrink_0(),
                                            )
                                            .child(sender.name())
                                    })
                                }
                            }))
                            .child(
                                div()
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                                    .child(ago),
                            )
                            .on_click(cx.listener(move |this, _, cx| {
                                this.action(id, cx);
                            }))
                    }))
                }
            })
        } else {
            div().children(self.render_skeleton(5))
        }
    }

    fn action(&self, id: u64, cx: &mut ViewContext<Self>) {
        cx.dispatch_action(Box::new(AddPanel {
            panel: PanelKind::Room(id),
            position: ui::dock::DockPlacement::Center,
        }))
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
                    .rounded(px(cx.theme().radius))
                    .text_xs()
                    .font_semibold()
                    .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
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
            .when(!self.is_collapsed, |this| this.child(self.render_item(cx)))
    }
}
