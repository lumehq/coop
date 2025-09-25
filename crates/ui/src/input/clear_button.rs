use gpui::{App, Styled};
use theme::ActiveTheme;

use crate::button::{Button, ButtonVariants};
use crate::{Icon, IconName, Sizable};

#[inline]
pub(crate) fn clear_button(cx: &App) -> Button {
    Button::new("clean")
        .icon(Icon::new(IconName::CloseCircle))
        .tooltip("Clear")
        .small()
        .transparent()
        .text_color(cx.theme().text_muted)
}
