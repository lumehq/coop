use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, relative, svg, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window,
};
use theme::ActiveTheme;

use crate::{h_flex, v_flex, Disableable, IconName, Selectable};

type OnClick = Option<Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>>;

/// A Checkbox element.
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    label: Option<SharedString>,
    checked: bool,
    disabled: bool,
    on_click: OnClick,
}

impl Checkbox {
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

    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Disableable for Checkbox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Checkbox {
    fn element_id(&self) -> &ElementId {
        &self.id
    }

    fn selected(self, selected: bool) -> Self {
        self.checked(selected)
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (color, icon_color) = if self.disabled {
            (cx.theme().ghost_element_disabled, cx.theme().text_muted)
        } else {
            (cx.theme().text_accent, cx.theme().surface_background)
        };

        h_flex()
            .id(self.id)
            .gap_2()
            .items_center()
            .line_height(relative(1.))
            .child(
                v_flex()
                    .relative()
                    .border_1()
                    .border_color(color)
                    .rounded_sm()
                    .size_4()
                    .flex_shrink_0()
                    .map(|this| match self.checked {
                        false => this.bg(cx.theme().ghost_element_background),
                        _ => this.bg(color),
                    })
                    .child(
                        svg()
                            .absolute()
                            .top_px()
                            .left_px()
                            .size_3()
                            .text_color(icon_color)
                            .map(|this| match self.checked {
                                true => this.path(IconName::Check.path()),
                                _ => this,
                            }),
                    ),
            )
            .map(|this| {
                if let Some(label) = self.label {
                    this.text_color(cx.theme().text_muted).child(
                        div()
                            .w_full()
                            .overflow_x_hidden()
                            .text_ellipsis()
                            .line_height(relative(1.))
                            .child(label),
                    )
                } else {
                    this
                }
            })
            .when(self.disabled, |this| {
                this.cursor_not_allowed()
                    .text_color(cx.theme().text_placeholder)
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_click(move |_, window, cx| {
                        let checked = !self.checked;
                        on_click(&checked, window, cx);
                    })
                },
            )
    }
}
