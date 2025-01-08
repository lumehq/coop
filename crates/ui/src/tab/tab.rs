use crate::theme::ActiveTheme;
use crate::Selectable;
use gpui::prelude::FluentBuilder;
use gpui::*;
use nostr_sdk::prelude::*;

#[derive(IntoElement)]
pub struct Tab {
    id: ElementId,
    base: Stateful<Div>,
    label: AnyElement,
    metadata: Option<Metadata>,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    disabled: bool,
    selected: bool,
}

impl Tab {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl IntoElement,
        metadata: Option<Metadata>,
    ) -> Self {
        let id: ElementId = id.into();

        Self {
            id: id.clone(),
            base: div().id(id).gap_1().py_1p5().px_3().h(px(30.)),
            label: label.into_any_element(),
            metadata,
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
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let (text_color, bg_color) = match (self.selected, self.disabled) {
            (true, false) => (cx.theme().tab_active_foreground, cx.theme().tab_active),
            (false, false) => (cx.theme().muted_foreground, cx.theme().tab),
            // disabled
            (true, true) => (cx.theme().muted_foreground, cx.theme().tab_active),
            (false, true) => (cx.theme().muted_foreground, cx.theme().tab),
        };

        self.base
            .flex()
            .items_center()
            .flex_shrink_0()
            .cursor_pointer()
            .overflow_hidden()
            .text_color(text_color)
            .bg(bg_color)
            .border_x_1()
            .border_color(cx.theme().transparent)
            .when(self.selected, |this| this.border_color(cx.theme().border))
            .text_sm()
            .when_some(self.prefix, |this, prefix| {
                this.child(prefix).text_color(text_color)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_ellipsis()
                    .text_xs()
                    .child(div().when_some(self.metadata, |this, metadata| {
                        if let Some(picture) = metadata.picture {
                            this.flex_shrink_0().child(
                                img(format!("https://wsrv.nl/?url={}&w=100&h=100&n=-1", picture))
                                    .size_4()
                                    .rounded_full()
                                    .object_fit(ObjectFit::Cover),
                            )
                        } else {
                            this.flex_shrink_0()
                                .child(img("brand/avatar.png").size_4().rounded_full())
                        }
                    }))
                    .child(self.label),
            )
            .when_some(self.suffix, |this, suffix| this.child(suffix))
    }
}
