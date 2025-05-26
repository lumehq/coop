use std::rc::Rc;

use gpui::{
    div, img, prelude::FluentBuilder, AnyElement, App, ClickEvent, Div, Img, InteractiveElement,
    IntoElement, ObjectFit, ParentElement as _, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, Window,
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
            base: div().h_9().w_full().px_1p5(),
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
            .child(
                div()
                    .flex_shrink_0()
                    .size_6()
                    .rounded_full()
                    .overflow_hidden()
                    .map(|this| {
                        if let Some(img_ele) = self.img {
                            this.child(
                                img_ele
                                    .size_full()
                                    .rounded_full()
                                    .object_fit(ObjectFit::Fill)
                                    .with_fallback(fallback_image),
                            )
                        } else {
                            this.child(fallback_image())
                        }
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when_some(self.label, |this, label| {
                        this.child(
                            div()
                                .flex_1()
                                .line_clamp(1)
                                .text_ellipsis()
                                .font_medium()
                                .child(label),
                        )
                    })
                    .when_some(self.description, |this, description| {
                        this.child(
                            div()
                                .flex_shrink_0()
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

fn fallback_image() -> AnyElement {
    img("brand/avatar.png")
        .rounded_full()
        .size_6()
        .into_any_element()
}
