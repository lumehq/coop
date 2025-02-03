use crate::{
    theme::{scale::ColorScaleStep, ActiveTheme as _},
    AxisExt as _,
};
use gpui::{
    div, prelude::FluentBuilder as _, px, App, Axis, Div, ElementId, InteractiveElement,
    IntoElement, ParentElement as _, Pixels, RenderOnce, Stateful, StatefulInteractiveElement,
    Styled as _, Window,
};

pub(crate) const HANDLE_PADDING: Pixels = px(8.);
pub(crate) const HANDLE_SIZE: Pixels = px(2.);

#[derive(IntoElement)]
pub(crate) struct ResizeHandle {
    base: Stateful<Div>,
    axis: Axis,
}

impl ResizeHandle {
    fn new(id: impl Into<ElementId>, axis: Axis) -> Self {
        Self {
            base: div().id(id.into()),
            axis,
        }
    }
}

/// Create a resize handle for a resizable panel.
pub(crate) fn resize_handle(id: impl Into<ElementId>, axis: Axis) -> ResizeHandle {
    ResizeHandle::new(id, axis)
}

impl InteractiveElement for ResizeHandle {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for ResizeHandle {}

impl RenderOnce for ResizeHandle {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.base
            .occlude()
            .absolute()
            .flex_shrink_0()
            .when(self.axis.is_horizontal(), |this| {
                this.cursor_col_resize()
                    .top_0()
                    .left(px(-1.))
                    .w(HANDLE_SIZE)
                    .h_full()
                    .pt_12()
                    .pb_4()
            })
            .when(self.axis.is_vertical(), |this| {
                this.cursor_row_resize()
                    .top(px(-1.))
                    .left_0()
                    .w_full()
                    .h(HANDLE_SIZE)
                    .px_6()
            })
            .child(
                div()
                    .rounded_full()
                    .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::SIX)))
                    .when(self.axis.is_horizontal(), |this| {
                        this.h_full().w(HANDLE_SIZE)
                    })
                    .when(self.axis.is_vertical(), |this| this.w_full().h(HANDLE_SIZE)),
            )
    }
}
