use crate::theme::{scale::ColorScaleStep, ActiveTheme};
use gpui::{
    div, prelude::FluentBuilder, px, relative, App, IntoElement, ParentElement, RenderOnce, Styled,
    Window,
};

/// A Progress bar element.
#[derive(IntoElement)]
pub struct Progress {
    value: f32,
    height: f32,
}

impl Progress {
    pub fn new() -> Self {
        Progress {
            value: Default::default(),
            height: 8.,
        }
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Progress {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let rounded = px(self.height / 2.);
        let relative_w = relative(match self.value {
            v if v < 0. => 0.,
            v if v > 100. => 1.,
            v => v / 100.,
        });

        div()
            .relative()
            .h(px(self.height))
            .rounded(rounded)
            .bg(cx.theme().accent.step(cx, ColorScaleStep::THREE))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(relative_w)
                    .bg(cx.theme().accent.step(cx, ColorScaleStep::NINE))
                    .map(|this| match self.value {
                        v if v >= 100. => this.rounded(rounded),
                        _ => this.rounded_l(rounded),
                    }),
            )
    }
}
