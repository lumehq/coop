use crate::views::sidebar::inbox::Inbox;
use compose::Compose;
use gpui::{
    div, px, AnyElement, AppContext, Entity, EntityId, EventEmitter, FocusHandle, FocusableView,
    IntoElement, ParentElement, Render, SharedString, Styled, View, ViewContext, VisualContext,
    WindowContext,
};
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
            let selected = compose.model.read(cx).selected(cx);
            let label = if selected.len() > 1 {
                "Create Group DM"
            } else {
                "Create DM"
            };

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
                                .w_full(),
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
                v_flex().px_2().gap_0p5().child(
                    Button::new("compose")
                        .small()
                        .ghost()
                        .not_centered()
                        .icon(Icon::new(IconName::ComposeFill))
                        .label("Compose")
                        .on_click(cx.listener(|this, _, cx| this.show_compose(cx))),
                ),
            )
            .child(self.inbox.clone())
    }
}
