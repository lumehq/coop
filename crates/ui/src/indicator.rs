use std::time::Duration;

use crate::{Icon, IconName, Sizable, Size};
use gpui::{
    div, ease_in_out, percentage, prelude::FluentBuilder as _, Animation, AnimationExt as _, Hsla,
    IntoElement, ParentElement, RenderOnce, Styled as _, Transformation, WindowContext,
};

#[derive(IntoElement)]
pub struct Indicator {
    size: Size,
    icon: Icon,
    speed: Duration,
    color: Option<Hsla>,
}

impl Indicator {
    pub fn new() -> Self {
        Self {
            size: Size::Medium,
            speed: Duration::from_secs_f64(0.8),
            icon: Icon::new(IconName::Loader),
            color: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl Default for Indicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Indicator {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Indicator {
    fn render(self, _: &mut WindowContext) -> impl IntoElement {
        div()
            .child(
                self.icon
                    .with_size(self.size)
                    .when_some(self.color, |this, color| this.text_color(color))
                    .with_animation(
                        "circle",
                        Animation::new(self.speed).repeat().with_easing(ease_in_out),
                        |this, delta| this.transform(Transformation::rotate(percentage(delta))),
                    ),
            )
            .into_element()
    }
}