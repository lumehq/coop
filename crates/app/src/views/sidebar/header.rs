use std::rc::Rc;

use gpui::{
    div, percentage, prelude::FluentBuilder, px, App, ClickEvent, Div, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use ui::{
    theme::{scale::ColorScaleStep, ActiveTheme},
    Collapsible, Icon, IconName, StyledExt,
};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Header {
    base: Div,
    label: SharedString,
    icon: Icon,
    collapsed: bool,
    handler: Handler,
}

impl Header {
    pub fn new(label: impl Into<SharedString>, icon: impl Into<Icon>) -> Self {
        Self {
            base: div(),
            label: label.into(),
            icon: icon.into(),
            collapsed: false,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Rc::new(handler);
        self
    }
}

impl Collapsible for Header {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl RenderOnce for Header {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handler = self.handler.clone();

        self.base
            .id("header")
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_0p5()
            .px_1()
            .h_6()
            .rounded(px(cx.theme().radius))
            .text_xs()
            .text_color(cx.theme().base.step(cx, ColorScaleStep::TEN))
            .font_semibold()
            .child(
                Icon::new(IconName::ChevronDown)
                    .size_6()
                    .when(self.collapsed, |this| this.rotate(percentage(270. / 360.))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.icon.size_3())
                    .child(self.label.clone()),
            )
            .on_click(move |ev, window, cx| handler(ev, window, cx))
            .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
    }
}
