use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder, px, App, ClickEvent, Div, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
use ui::{
    theme::{scale::ColorScaleStep, ActiveTheme},
    Icon,
};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct SidebarButton {
    base: Div,
    label: SharedString,
    icon: Option<Icon>,
    handler: Handler,
}

impl SidebarButton {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            base: div().flex().items_center().gap_3().px_3().h_8(),
            label: label.into(),
            icon: None,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Rc::new(handler);
        self
    }
}

impl RenderOnce for SidebarButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handler = self.handler.clone();

        self.base
            .id(self.label.clone())
            .rounded(px(cx.theme().radius))
            .when_some(self.icon, |this, icon| this.child(icon))
            .child(self.label.clone())
            .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
            .on_click(move |ev, window, cx| handler(ev, window, cx))
    }
}
