use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, ScrollHandle,
    StatefulInteractiveElement as _, Styled, Window,
};
use smallvec::SmallVec;

use crate::h_flex;

#[derive(IntoElement)]
pub struct TabBar {
    base: Div,
    id: ElementId,
    scroll_handle: ScrollHandle,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    children: SmallVec<[AnyElement; 2]>,
}

impl TabBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().px(px(-1.)),
            id: id.into(),
            children: SmallVec::new(),
            scroll_handle: ScrollHandle::new(),
            prefix: None,
            suffix: None,
        }
    }

    /// Track the scroll of the TabBar
    pub fn track_scroll(mut self, scroll_handle: ScrollHandle) -> Self {
        self.scroll_handle = scroll_handle;
        self
    }

    /// Set the prefix element of the TabBar
    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    /// Set the suffix element of the TabBar
    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }
}

impl ParentElement for TabBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Styled for TabBar {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for TabBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.base
            .id(self.id)
            .group("tab-bar")
            .relative()
            .px_1()
            .flex()
            .flex_none()
            .items_center()
            .when_some(self.prefix, |this, prefix| this.child(prefix))
            .child(
                h_flex()
                    .id("tabs")
                    .flex_grow()
                    .gap_1()
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll_handle)
                    .children(self.children),
            )
            .when_some(self.suffix, |this, suffix| this.child(suffix))
    }
}
