use crate::theme::{scale::ColorScaleStep, ActiveTheme};
use gpui::{
    div, px, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window,
};

pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self { text: text.into() })
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            // Wrap in a child, to ensure the left margin is applied to the tooltip
            div()
                .font_family(".SystemUIFont")
                .m_3()
                .bg(cx.theme().base.step(cx, ColorScaleStep::TWELVE))
                .text_color(cx.theme().base.step(cx, ColorScaleStep::ONE))
                .shadow_md()
                .rounded(px(6.))
                .py_0p5()
                .px_2()
                .text_sm()
                .child(self.text.clone()),
        )
    }
}
