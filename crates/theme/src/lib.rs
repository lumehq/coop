use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use gpui::{px, App, Global, Pixels, SharedString, Window};

mod colors;
mod registry;
mod scale;
mod scrollbar_mode;
mod theme;

pub use colors::*;
pub use registry::*;
pub use scale::*;
pub use scrollbar_mode::*;
pub use theme::*;

/// Defines window border radius for platforms that use client side decorations.
pub const CLIENT_SIDE_DECORATION_ROUNDING: Pixels = px(10.0);

/// Defines window shadow size for platforms that use client side decorations.
pub const CLIENT_SIDE_DECORATION_SHADOW: Pixels = px(10.0);

pub fn init(cx: &mut App) {
    registry::init(cx);

    Theme::sync_system_appearance(None, cx);
    Theme::sync_scrollbar_appearance(cx);
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    #[inline(always)]
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme colors
    pub colors: ThemeColors,

    /// Theme family
    pub theme: Rc<ThemeFamily>,

    /// The appearance of the theme (light or dark).
    pub mode: ThemeMode,

    /// The font family for the application.
    pub font_family: SharedString,

    /// The root font size for the application, default is 15px.
    pub font_size: Pixels,

    /// Radius for the general elements.
    pub radius: Pixels,

    /// Radius for the large elements, e.g.: modal, notification.
    pub radius_lg: Pixels,

    /// Enable shadow for the general elements. default is true
    pub shadow: bool,

    /// Show the scrollbar mode, default: scrolling
    pub scrollbar_mode: ScrollbarMode,
}

impl Deref for Theme {
    type Target = ThemeColors;

    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.colors
    }
}

impl Global for Theme {}

impl Theme {
    /// Returns the global theme reference
    pub fn global(cx: &App) -> &Theme {
        cx.global::<Theme>()
    }

    /// Returns the global theme mutable reference
    pub fn global_mut(cx: &mut App) -> &mut Theme {
        cx.global_mut::<Theme>()
    }

    /// Returns true if the theme is dark.
    pub fn is_dark(&self) -> bool {
        self.mode.is_dark()
    }

    /// Sync the theme with the system appearance
    pub fn sync_system_appearance(window: Option<&mut Window>, cx: &mut App) {
        let appearance = window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance());

        Self::change(appearance, window, cx);
    }

    /// Sync the Scrollbar showing behavior with the system
    pub fn sync_scrollbar_appearance(cx: &mut App) {
        Theme::global_mut(cx).scrollbar_mode = if cx.should_auto_hide_scrollbars() {
            ScrollbarMode::Scrolling
        } else {
            ScrollbarMode::Hover
        };
    }

    /// Apply a new theme to the application.
    pub fn apply_theme(new_theme: Rc<ThemeFamily>, window: Option<&mut Window>, cx: &mut App) {
        let theme = cx.global_mut::<Theme>();
        let mode = theme.mode;
        // Update the theme
        theme.theme = new_theme;
        // Emit a theme change event
        Self::change(mode, window, cx);
    }

    /// Change the app's appearance
    pub fn change<M>(mode: M, window: Option<&mut Window>, cx: &mut App)
    where
        M: Into<ThemeMode>,
    {
        if !cx.has_global::<Theme>() {
            let default_theme = ThemeFamily::default();
            let theme = Theme::from(default_theme);

            cx.set_global(theme);
        }

        let mode = mode.into();
        let theme = cx.global_mut::<Theme>();

        // Set the theme mode
        theme.mode = mode;

        // Set the theme colors
        if mode.is_dark() {
            theme.colors = *theme.theme.dark();
        } else {
            theme.colors = *theme.theme.light();
        }

        // Refresh the window if available
        if let Some(window) = window {
            window.refresh();
        }
    }
}

impl From<ThemeFamily> for Theme {
    fn from(family: ThemeFamily) -> Self {
        let mode = ThemeMode::default();
        // Define the theme colors based on the appearance
        let colors = match mode {
            ThemeMode::Light => family.light(),
            ThemeMode::Dark => family.dark(),
        };

        Theme {
            font_size: px(15.),
            font_family: ".SystemUIFont".into(),
            radius: px(6.),
            radius_lg: px(12.),
            shadow: true,
            scrollbar_mode: ScrollbarMode::default(),
            mode,
            colors: *colors,
            theme: Rc::new(family),
        }
    }
}
