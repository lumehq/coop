use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder, App, ClickEvent, Div, Img, InteractiveElement, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
use theme::ActiveTheme;
use ui::StyledExt;

#[derive(IntoElement)]
pub struct DisplayRoom {
    ix: usize,
    base: Div,
    img: Option<Img>,
    label: Option<SharedString>,
    description: Option<SharedString>,
    #[allow(clippy::type_complexity)]
    handler: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl DisplayRoom {
    pub fn new(ix: usize) -> Self {
        Self {
            ix,
            base: div().h_8().w_full().px_2(),
            img: None,
            label: None,
            description: None,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn img(mut self, img: Img) -> Self {
        self.img = Some(img);
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

impl RenderOnce for DisplayRoom {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handler = self.handler.clone();

        self.base
            .id(self.ix)
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .rounded(cx.theme().radius)
            .child(div().size_6().flex_none().map(|this| {
                if let Some(img) = self.img {
                    this.child(img.size_6().flex_none())
                } else {
                    this.child(
                        div()
                            .size_6()
                            .flex_none()
                            .flex()
                            .justify_center()
                            .items_center()
                            .rounded_full()
                            .bg(cx.theme().element_background),
                    )
                }
            }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when_some(self.label, |this, label| {
                        this.child(
                            div()
                                .line_clamp(1)
                                .text_ellipsis()
                                .font_medium()
                                .child(label),
                        )
                    })
                    .when_some(self.description, |this, description| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().text_placeholder)
                                .child(description),
                        )
                    }),
            )
            .hover(|this| this.bg(cx.theme().elevated_surface_background))
            .on_click(move |ev, window, cx| handler(ev, window, cx))
    }
}
