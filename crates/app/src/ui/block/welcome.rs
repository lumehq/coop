use components::{
    theme::{ActiveTheme, Colorize},
    StyledExt,
};
use gpui::*;

use super::Block;

pub struct WelcomeBlock {
    focus_handle: FocusHandle,
}

impl WelcomeBlock {
    pub fn view(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::new)
    }

    fn new(cx: &mut ViewContext<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Block for WelcomeBlock {
    fn title() -> &'static str {
        "Welcome"
    }

    fn new_view(cx: &mut WindowContext) -> View<impl FocusableView> {
        Self::view(cx)
    }

    fn zoomable() -> bool {
        false
    }
}

impl FocusableView for WelcomeBlock {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WelcomeBlock {
    fn render(&mut self, cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("coop on nostr.")
            .text_color(cx.theme().muted.darken(0.1))
            .font_black()
            .text_sm()
    }
}
