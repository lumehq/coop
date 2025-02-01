use gpui::{div, svg, Context, IntoElement, ParentElement, Render, Styled, Window};
use ui::theme::{scale::ColorScaleStep, ActiveTheme};

pub struct Startup {}

impl Startup {
    pub fn new(_window: &mut Window, _cx: &mut Context<'_, Self>) -> Self {
        Self {}
    }
}

impl Render for Startup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("brand/coop.svg")
                    .size_12()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE)),
            )
    }
}
