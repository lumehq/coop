use crate::{
    button::{Button, ButtonVariants as _},
    theme::{scale::ColorScaleStep, ActiveTheme as _},
    Icon, IconName, Sizable as _,
};
use gpui::{App, Styled, Window};

pub(crate) struct ClearButton {}

impl ClearButton {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(window: &mut Window, cx: &mut App) -> Button {
        Button::new("clean")
            .icon(
                Icon::new(IconName::CircleX)
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN)),
            )
            .ghost()
            .xsmall()
    }
}
