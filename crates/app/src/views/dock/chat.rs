use coop_ui::block::Block;
use gpui::*;

pub struct ChatBlock {
    focus_handle: FocusHandle,
}

impl ChatBlock {
    pub fn view(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::new)
    }

    fn new(cx: &mut ViewContext<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Block for ChatBlock {
    fn title() -> &'static str {
        "Chat"
    }

    fn new_view(cx: &mut WindowContext) -> View<impl FocusableView> {
        Self::view(cx)
    }

    fn zoomable() -> bool {
        false
    }
}

impl FocusableView for ChatBlock {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatBlock {
    fn render(&mut self, _cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("Test")
    }
}
