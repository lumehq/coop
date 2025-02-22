use chats::{registry::ChatRegistry, room::Room};
use compose::Compose;
use gpui::{
    div, img, percentage, prelude::FluentBuilder, px, uniform_list, AnyElement, App, AppContext,
    Context, Div, Empty, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Window,
};
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Disableable, Icon, IconName, Sizable, StyledExt,
};

use super::app::AddPanel;

mod compose;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    Sidebar::new(window, cx)
}

pub struct Sidebar {
    name: SharedString,
    focus_handle: FocusHandle,
    label: SharedString,
    is_collapsed: bool,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let label = SharedString::from("Inbox");

        Self {
            name: "Sidebar".into(),
            is_collapsed: false,
            focus_handle,
            label,
        }
    }

    fn render_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let compose = cx.new(|cx| Compose::new(window, cx));

        window.open_modal(cx, move |modal, window, cx| {
            let label = compose.read(cx).label(window, cx);
            let is_submitting = compose.read(cx).is_submitting();

            modal
                .title("Direct Messages")
                .width(px(420.))
                .child(compose.clone())
                .footer(
                    div()
                        .p_2()
                        .border_t_1()
                        .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                        .child(
                            Button::new("create_dm_btn")
                                .label(label)
                                .primary()
                                .bold()
                                .rounded(ButtonRounded::Large)
                                .w_full()
                                .loading(is_submitting)
                                .disabled(is_submitting)
                                .on_click(window.listener_for(&compose, |this, _, window, cx| {
                                    this.compose(window, cx)
                                })),
                        ),
                )
        })
    }

    fn render_room(&self, ix: usize, room: &Entity<Room>, cx: &Context<Self>) -> Stateful<Div> {
        let room = room.read(cx);

        div()
            .id(ix)
            .px_1()
            .h_8()
            .w_full()
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
                    this.when_some(room.members.read(cx).first(), |this, member| {
                        this.flex()
                            .items_center()
                            .gap_2()
                            .child(img(member.avatar()).size_6().rounded_full().flex_shrink_0())
                            .child(member.name())
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
                    this.open(id, window, cx);
                })
            })
    }

    fn open(&self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(
            Box::new(AddPanel::new(
                super::app::PanelKind::Room(id),
                ui::dock_area::dock::DockPlacement::Center,
            )),
            cx,
        );
    }
}

impl Panel for Sidebar {
    fn panel_id(&self) -> SharedString {
        "Sidebar".into()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }
}

impl EventEmitter<PanelEvent> for Sidebar {}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .px_2()
                    .py_3()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("new_message")
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_1()
                            .h_7()
                            .text_xs()
                            .font_semibold()
                            .rounded(px(cx.theme().radius))
                            .child(
                                div()
                                    .size_6()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(cx.theme().accent.step(cx, ColorScaleStep::NINE))
                                    .child(
                                        Icon::new(IconName::ComposeFill)
                                            .small()
                                            .text_color(cx.theme().base.darken(cx)),
                                    ),
                            )
                            .child("New Message")
                            .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                // Open compose modal
                                this.render_compose(window, cx);
                            })),
                    )
                    .child(Empty),
            )
            .child(
                div()
                    .px_2()
                    .w_full()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("inbox_header")
                            .px_1()
                            .h_7()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
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
                        this.flex_1()
                            .w_full()
                            .when_some(ChatRegistry::global(cx), |this, state| {
                                let rooms = state.read(cx).rooms();
                                let len = rooms.len();

                                this.child(
                                    uniform_list(
                                        entity,
                                        "rooms",
                                        len,
                                        move |this, range, _, cx| {
                                            let mut items = vec![];

                                            for ix in range {
                                                if let Some(room) = rooms.get(ix) {
                                                    items.push(this.render_room(ix, room, cx));
                                                }
                                            }

                                            items
                                        },
                                    )
                                    .size_full(),
                                )
                            })
                    }),
            )
    }
}
