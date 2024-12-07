use components::{scroll::ScrollbarAxis, StyledExt};
use coop_ui::block::Block;
use gpui::*;

use super::inbox::Inbox;

pub struct LeftDock {
    inbox: View<Inbox>,
    focus_handle: FocusHandle,
    view_id: EntityId,
}

impl LeftDock {
    pub fn view(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::new)
    }

    fn new(cx: &mut ViewContext<Self>) -> Self {
        let inbox = cx.new_view(Inbox::new);

        Self {
            inbox,
            focus_handle: cx.focus_handle(),
            view_id: cx.view().entity_id(),
        }
    }
}

impl Block for LeftDock {
    fn title() -> &'static str {
        "Left Dock"
    }

    fn new_view(cx: &mut WindowContext) -> View<impl FocusableView> {
        Self::view(cx)
    }

    fn zoomable() -> bool {
        false
    }
}

impl FocusableView for LeftDock {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
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
