use gpui::{
    div, relative, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use theme::ActiveTheme;

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
            div()
                .font_family(".SystemUIFont")
                .m_3()
                .p_2()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().surface_background)
                .shadow_md()
                .rounded_lg()
                .text_sm()
                .text_color(cx.theme().text_muted)
                .line_height(relative(1.25))
                .child(self.text.clone()),
        )
    }
}
