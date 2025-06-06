use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Stateful, StatefulInteractiveElement, Styled, Window,
};
use theme::ActiveTheme;

use crate::Selectable;

pub mod tab_bar;

#[derive(IntoElement)]
pub struct Tab {
    id: ElementId,
    base: Stateful<Div>,
    label: AnyElement,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    disabled: bool,
    selected: bool,
}

impl Tab {
    pub fn new(id: impl Into<ElementId>, label: impl IntoElement) -> Self {
        let id: ElementId = id.into();

        Self {
            id: id.clone(),
            base: div().id(id),
            label: label.into_any_element(),
            disabled: false,
            selected: false,
            prefix: None,
            suffix: None,
        }
    }

    /// Set the left side of the tab
    pub fn prefix(mut self, prefix: impl Into<AnyElement>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Set the right side of the tab
    pub fn suffix(mut self, suffix: impl Into<AnyElement>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    /// Set disabled state to the tab
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Tab {
    fn element_id(&self) -> &ElementId {
        &self.id
    }

    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl InteractiveElement for Tab {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Tab {}

impl Styled for Tab {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Tab {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (text_color, bg_color, hover_bg_color) = match (self.selected, self.disabled) {
            (true, false) => (
                cx.theme().text,
                cx.theme().tab_active_background,
                cx.theme().tab_hover_background,
            ),
            (false, false) => (
                cx.theme().text_muted,
                cx.theme().ghost_element_background,
                cx.theme().tab_hover_background,
            ),
            (true, true) => (
                cx.theme().text_muted,
                cx.theme().ghost_element_background,
                cx.theme().tab_hover_background,
            ),
            (false, true) => (
                cx.theme().text_muted,
                cx.theme().ghost_element_background,
                cx.theme().tab_hover_background,
            ),
        };

        self.base
            .h(px(30.))
            .px_2()
            .relative()
            .flex()
            .items_center()
            .flex_shrink_0()
            .cursor_pointer()
            .overflow_hidden()
            .text_xs()
            .text_ellipsis()
            .text_color(text_color)
            .bg(bg_color)
            .rounded(cx.theme().radius)
            .hover(|this| this.bg(hover_bg_color))
            .when_some(self.prefix, |this, prefix| {
                this.child(prefix).text_color(text_color)
            })
            .child(self.label)
            .when_some(self.suffix, |this, suffix| this.child(suffix))
    }
}
