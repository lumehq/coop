pub use event::InteractiveElementExt;
pub use focusable::FocusableCycle;
pub use icon::*;
pub use styled::*;
pub use svg_img::*;
pub use title_bar::*;
pub use window::{ContextModal, Root};

pub use crate::Disableable;

pub(crate) mod actions;
pub mod animation;
pub mod avatar;
pub mod button;
pub mod checkbox;
pub mod context_menu;
pub mod divider;
pub mod dock_area;
pub mod dropdown;
pub mod emoji_picker;
pub mod history;
pub mod indicator;
pub mod input;
pub mod list;
pub mod modal;
pub mod notification;
pub mod popover;
pub mod popup_menu;
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
mod styled;
mod svg_img;
mod title_bar;
mod window;

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
    popup_menu::init(cx);
}
