use crate::{
    button::{Button, ButtonVariants as _},
    theme::{scale::ColorScaleStep, ActiveTheme as _},
    Icon, IconName, Sizable as _,
};
use gpui::{Styled, WindowContext};

pub(crate) struct ClearButton {}

impl ClearButton {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(cx: &mut WindowContext) -> Button {
        Button::new("clean")
            .icon(
                Icon::new(IconName::CircleX)
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN)),
            )
            .ghost()
            .xsmall()
    }
}
