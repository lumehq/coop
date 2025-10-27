pub use event::InteractiveElementExt;
pub use focusable::FocusableCycle;
pub use icon::*;
pub use kbd::*;
pub use menu::{context_menu, popup_menu};
pub use root::{ContextModal, Root};
pub use styled::*;
pub use window_border::{window_border, WindowBorder};

pub use crate::Disableable;

pub mod actions;
pub mod animation;
pub mod avatar;
pub mod button;
pub mod checkbox;
pub mod divider;
pub mod dock_area;
pub mod dropdown;
pub mod emoji_picker;
pub mod history;
pub mod indicator;
pub mod input;
pub mod list;
pub mod menu;
pub mod modal;
pub mod notification;
pub mod popover;
pub mod resizable;
pub mod scroll;
pub mod skeleton;
pub mod switch;
pub mod tab;
pub mod text;
pub mod tooltip;

mod event;
mod focusable;
mod icon;
mod kbd;
mod root;
mod styled;
mod window_border;

i18n::init!();

/// Initialize the UI module.
///
/// This must be called before using any of the UI components.
/// You can initialize the UI module at your application's entry point.
pub fn init(cx: &mut gpui::App) {
    theme::init(cx);
    dropdown::init(cx);
    input::init(cx);
    list::init(cx);
    modal::init(cx);
    popover::init(cx);
    menu::init(cx);
}
