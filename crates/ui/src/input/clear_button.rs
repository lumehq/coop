use gpui::{App, Styled};
use i18n::t;
use theme::ActiveTheme;

use crate::button::{Button, ButtonVariants as _};
use crate::{Icon, IconName, Sizable as _};

#[inline]
pub(crate) fn clear_button(cx: &App) -> Button {
    Button::new("clean")
        .icon(Icon::new(IconName::CloseCircle))
        .tooltip(t!("common.clear"))
        .small()
        .text_color(cx.theme().text_muted)
        .transparent()
}
