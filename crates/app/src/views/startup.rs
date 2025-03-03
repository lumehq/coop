use gpui::{
    div, svg, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
};
use ui::theme::{scale::ColorScaleStep, ActiveTheme};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Startup> {
    Startup::new(window, cx)
}

pub struct Startup {}

impl Startup {
    pub fn new(_window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {})
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
