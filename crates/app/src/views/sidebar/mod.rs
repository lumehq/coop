use crate::views::sidebar::inbox::Inbox;
use compose::Compose;
use gpui::{
    div, px, AnyElement, AppContext, BorrowAppContext, Entity, EntityId, EventEmitter, FocusHandle,
    FocusableView, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, View, ViewContext, VisualContext, WindowContext,
};
use registry::chat::ChatRegistry;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::{
        panel::{Panel, PanelEvent},
        state::PanelState,
    },
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, ContextModal, Icon, IconName, Sizable, StyledExt,
};

mod compose;
mod inbox;

pub struct Sidebar {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Dock
    inbox: View<Inbox>,
    view_id: EntityId,
}

impl Sidebar {
    pub fn new(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::view)
    }

    fn view(cx: &mut ViewContext<Self>) -> Self {
        let inbox = cx.new_view(Inbox::new);

        Self {
            name: "Sidebar".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            view_id: cx.view().entity_id(),
            inbox,
        }
    }

    fn show_compose(&mut self, cx: &mut ViewContext<Self>) {
        let compose = cx.new_view(Compose::new);

        cx.open_modal(move |modal, cx| {
            let label = compose.read(cx).label(cx);

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
                            Button::new("create")
                                .label(label)
                                .primary()
                                .bold()
                                .rounded(ButtonRounded::Large)
                                .w_full()
                                .on_click(cx.listener_for(&compose, |this, _, cx| {
                                    if let Some(room) = this.room(cx) {
                                        cx.update_global::<ChatRegistry, _>(|this, cx| {
                                            this.new_room(room, cx);
                                        });

                                        cx.close_modal();
                                    }
                                })),
                        ),
                )
        })
    }
}

impl Panel for Sidebar {
    fn panel_id(&self) -> SharedString {
        "Sidebar".into()
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closeable(&self, _cx: &WindowContext) -> bool {
        self.closeable
    }

    fn zoomable(&self, _cx: &WindowContext) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &WindowContext) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _cx: &WindowContext) -> Vec<Button> {
        vec![]
    }

    fn dump(&self, _cx: &AppContext) -> PanelState {
        PanelState::new(self)
    }
}

impl EventEmitter<PanelEvent> for Sidebar {}

impl FocusableView for Sidebar {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .scrollable(self.view_id, ScrollbarAxis::Vertical)
            .py_3()
            .gap_3()
            .child(
                v_flex().px_2().gap_1().child(
                    div()
                        .id("new")
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
                        .on_click(cx.listener(|this, _, cx| this.show_compose(cx))),
                ),
            )
            .child(self.inbox.clone())
    }
}
