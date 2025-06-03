use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, Axis, Div, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
};
use theme::ActiveTheme;

/// A divider that can be either vertical or horizontal.
#[derive(IntoElement)]
pub struct Divider {
    base: Div,
    label: Option<SharedString>,
    axis: Axis,
    color: Option<Hsla>,
}

impl Divider {
    pub fn vertical() -> Self {
        Self {
            base: div().h_full(),
            axis: Axis::Vertical,
            label: None,
            color: None,
        }
    }

    pub fn horizontal() -> Self {
        Self {
            base: div().w_full(),
            axis: Axis::Horizontal,
            label: None,
            color: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl Styled for Divider {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Divider {
    fn render(self, _window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        self.base
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .rounded_full()
                    .map(|this| match self.axis {
                        Axis::Vertical => this.w(px(2.)).h_full(),
                        Axis::Horizontal => this.h(px(2.)).w_full(),
                    })
                    .bg(self.color.unwrap_or(cx.theme().border_variant)),
            )
            .when_some(self.label, |this, label| {
                this.child(
                    div()
                        .px_2()
                        .py_1()
                        .mx_auto()
                        .text_xs()
                        .bg(cx.theme().background)
                        .child(label),
                )
            })
    }
}
