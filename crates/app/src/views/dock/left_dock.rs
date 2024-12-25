use coop_ui::{
    button::Button,
    dock::{Panel, PanelEvent, PanelState},
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    StyledExt,
};
use gpui::*;

use super::inbox::Inbox;

pub struct LeftDock {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Dock
    inbox: View<Inbox>,
    view_id: EntityId,
}

impl LeftDock {
    pub fn new(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::view)
    }

    fn view(cx: &mut ViewContext<Self>) -> Self {
        let inbox = cx.new_view(Inbox::new);

        Self {
            name: "Left Dock".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            view_id: cx.view().entity_id(),
            inbox,
        }
    }
}

impl Panel for LeftDock {
    fn panel_name(&self) -> SharedString {
        "LeftDock".into()
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

impl EventEmitter<PanelEvent> for LeftDock {}

impl FocusableView for LeftDock {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LeftDock {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .child(self.inbox.clone())
            .scrollable(self.view_id, ScrollbarAxis::Vertical)
    }
}
