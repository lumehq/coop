use gpui::{Styled, WindowContext};

use crate::{
    button::{Button, ButtonVariants as _},
    theme::ActiveTheme as _,
    Icon, IconName, Sizable as _,
};

pub(crate) struct ClearButton {}

impl ClearButton {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(cx: &mut WindowContext) -> Button {
        Button::new("clean")
            .icon(Icon::new(IconName::CircleX).text_color(cx.theme().muted_foreground))
            .ghost()
            .xsmall()
    }
}
