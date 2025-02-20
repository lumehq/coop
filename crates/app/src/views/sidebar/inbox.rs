use crate::views::app::{AddPanel, PanelKind};
use account::registry::Account;
use chats::registry::ChatRegistry;
use gpui::{
    div, img, percentage, prelude::FluentBuilder, px, relative, Context, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    TextAlign, Window,
};
use ui::{
    dock_area::dock::DockPlacement,
    skeleton::Skeleton,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, Collapsible, Icon, IconName, StyledExt,
};

pub struct Inbox {
    label: SharedString,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(_window: &mut Window, _cx: &mut Context<'_, Self>) -> Self {
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

    fn render_item(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(chats) = ChatRegistry::global(cx) {
            div().map(|this| {
                let state = chats.read(cx);
                let rooms = state.rooms();

                if state.is_loading() {
                    this.children(self.render_skeleton(5))
                } else if rooms.is_empty() {
                    this.px_1()
                        .w_full()
                        .h_20()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .text_align(TextAlign::Center)
                        .rounded(px(cx.theme().radius))
                        .bg(cx.theme().base.step(cx, ColorScaleStep::THREE))
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .line_height(relative(1.2))
                                .child("No chats"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                                .child("Recent chats will appear here."),
                        )
                } else {
                    this.children(rooms.iter().map(|model| {
                        let room = model.read(cx);
                        let room_id: SharedString = room.id.to_string().into();

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
                            .child(div().flex_1().truncate().font_medium().map(|this| {
                                if room.is_group(cx) {
                                    this.flex()
                                        .items_center()
                                        .gap_2()
                                        .child(img("brand/avatar.png").size_6().rounded_full())
                                        .when_some(room.name(cx), |this, name| this.child(name))
                                } else {
                                    this.when_some(Account::global(cx), |this, account| {
                                        let user_profile = account.read(cx).get();

                                        this.when_some(
                                            room.members
                                                .read(cx)
                                                .iter()
                                                .find(|&m| m != user_profile),
                                            |this, member| {
                                                this.flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        img(member.avatar())
                                                            .size_6()
                                                            .rounded_full()
                                                            .flex_shrink_0(),
                                                    )
                                                    .child(member.name())
                                            },
                                        )
                                    })
                                }
                            }))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                                    .child(room.last_seen.ago()),
                            )
                            .on_click({
                                let id = room.id;
                                cx.listener(move |this, _, window, cx| {
                                    this.action(id, window, cx);
                                })
                            })
                    }))
                }
            })
        } else {
            div().children(self.render_skeleton(5))
        }
    }

    fn action(&self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(
            Box::new(AddPanel::new(PanelKind::Room(id), DockPlacement::Center)),
            cx,
        );
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size_6()
                            .when(self.is_collapsed, |this| {
                                this.rotate(percentage(270. / 360.))
                            }),
                    )
                    .child(self.label.clone())
                    .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
                    .on_click(cx.listener(move |view, _event, _window, cx| {
                        view.is_collapsed = !view.is_collapsed;
                        cx.notify();
                    })),
            )
            .when(!self.is_collapsed, |this| {
                this.child(self.render_item(window, cx))
            })
    }
}
