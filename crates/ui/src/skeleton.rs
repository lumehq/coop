use std::time::Duration;

use gpui::{
    bounce, div, ease_in_out, Animation, AnimationExt, IntoElement, RenderOnce, StyleRefinement,
    Styled,
};
use theme::ActiveTheme;

use crate::StyledExt;

#[derive(IntoElement)]
pub struct Skeleton {
    style: StyleRefinement,
    secondary: bool,
}

impl Skeleton {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            secondary: false,
        }
    }

    pub fn secondary(mut self, secondary: bool) -> Self {
        self.secondary = secondary;
        self
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Skeleton {
    fn render(self, _window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let color = if self.secondary {
            cx.theme().ghost_element_active.opacity(0.5)
        } else {
            cx.theme().ghost_element_active
        };

        div()
            .w_full()
            .h_4()
            .rounded_md()
            .refine_style(&self.style)
            .bg(color)
            .with_animation(
                "skeleton",
                Animation::new(Duration::from_secs(2))
                    .repeat()
                    .with_easing(bounce(ease_in_out)),
                move |this, delta| {
                    let v = 1.0 - delta * 0.5;
                    this.opacity(v)
                },
            )
    }
}
