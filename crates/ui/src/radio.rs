use crate::{
    h_flex,
    theme::{scale::ColorScaleStep, ActiveTheme},
    IconName,
};
use gpui::{
    div, prelude::FluentBuilder, relative, svg, ElementId, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, WindowContext,
};

type OnClick = Option<Box<dyn Fn(&bool, &mut WindowContext) + 'static>>;

/// A Radio element.
///
/// This is not included the Radio group implementation, you can manage the group by yourself.
#[derive(IntoElement)]
pub struct Radio {
    id: ElementId,
    label: Option<SharedString>,
    checked: bool,
    disabled: bool,
    on_click: OnClick,
}

impl Radio {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            checked: false,
            disabled: false,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&bool, &mut WindowContext) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Radio {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let color = if self.disabled {
            cx.theme().accent.step(cx, ColorScaleStep::FIVE)
        } else {
            cx.theme().accent.step(cx, ColorScaleStep::NINE)
        };

        h_flex()
            .id(self.id)
            .gap_x_2()
            .items_center()
            .line_height(relative(1.))
            .child(
                div()
                    .relative()
                    .size_4()
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(color)
                    .when(self.checked, |this| this.bg(color))
                    .child(
                        svg()
                            .absolute()
                            .top_px()
                            .left_px()
                            .size_3()
                            .text_color(color)
                            .map(|this| match self.checked {
                                true => this.path(IconName::Check.path()),
                                false => this,
                            }),
                    ),
            )
            .when_some(self.label, |this, label| {
                this.child(
                    div()
                        .size_full()
                        .overflow_x_hidden()
                        .text_ellipsis()
                        .line_height(relative(1.))
                        .child(label),
                )
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_click(move |_event, cx| {
                        on_click(&!self.checked, cx);
                    })
                },
            )
    }
}
