use crate::scroll::ScrollbarShow;
use colors::{default_color_scales, hsl};
use gpui::{App, Global, Hsla, SharedString, Window, WindowAppearance};
use scale::ColorScaleSet;
use std::ops::{Deref, DerefMut};

pub mod colors;
pub mod scale;

pub fn init(cx: &mut App) {
    Theme::sync_system_appearance(None, cx)
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    #[inline]
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemColors {
    pub background: Hsla,
    pub transparent: Hsla,
    pub scrollbar: Hsla,
    pub scrollbar_thumb: Hsla,
    pub scrollbar_thumb_hover: Hsla,
    pub window_border: Hsla,
    pub danger: Hsla,
}

impl SystemColors {
    pub fn light() -> Self {
        Self {
            background: hsl(0.0, 0.0, 100.),
            transparent: Hsla::transparent_black(),
            window_border: hsl(240.0, 5.9, 78.0),
            scrollbar: hsl(0., 0., 97.).opacity(0.75),
            scrollbar_thumb: hsl(0., 0., 69.).opacity(0.9),
            scrollbar_thumb_hover: hsl(0., 0., 59.),
            danger: hsl(0.0, 84.2, 60.2),
        }
    }

    pub fn dark() -> Self {
        Self {
            background: hsl(0.0, 0.0, 8.0),
            transparent: Hsla::transparent_black(),
            window_border: hsl(240.0, 3.7, 28.0),
            scrollbar: hsl(240., 1., 15.).opacity(0.75),
            scrollbar_thumb: hsl(0., 0., 48.).opacity(0.9),
            scrollbar_thumb_hover: hsl(0., 0., 68.),
            danger: hsl(0.0, 62.8, 30.6),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

impl Appearance {
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }
}

pub struct Theme {
    colors: SystemColors,
    /// Base colors.
    pub base: ColorScaleSet,
    /// Accent colors.
    pub accent: ColorScaleSet,
    /// Window appearances.
    pub appearance: Appearance,
    pub font_family: SharedString,
    pub font_size: f32,
    pub radius: f32,
    pub shadow: bool,
    /// Show the scrollbar mode, default: Scrolling
    pub scrollbar_show: ScrollbarShow,
}

impl Deref for Theme {
    type Target = SystemColors;

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

    /// Sync the theme with the system appearance
    pub fn sync_system_appearance(window: Option<&mut Window>, cx: &mut App) {
        match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                Self::change(Appearance::Dark, window, cx)
            }
            WindowAppearance::Light | WindowAppearance::VibrantLight => {
                Self::change(Appearance::Light, window, cx)
            }
        }
    }

    pub fn change(mode: Appearance, window: Option<&mut Window>, cx: &mut App) {
        let theme = Theme::new(mode);
        cx.set_global(theme);

        if let Some(window) = window {
            window.refresh();
        }
    }
}

impl Theme {
    fn new(appearance: Appearance) -> Self {
        let color_scales = default_color_scales();
        let colors = match appearance {
            Appearance::Light => SystemColors::light(),
            Appearance::Dark => SystemColors::dark(),
        };

        Theme {
            base: color_scales.gray,
            accent: color_scales.yellow,
            font_size: 15.0,
            font_family: if cfg!(target_os = "macos") {
                ".SystemUIFont".into()
            } else if cfg!(target_os = "windows") {
                "Segoe UI".into()
            } else {
                "FreeMono".into()
            },
            radius: 5.0,
            shadow: false,
            scrollbar_show: ScrollbarShow::default(),
            appearance,
            colors,
        }
    }
}
