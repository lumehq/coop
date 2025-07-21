use std::ops::{Deref, DerefMut};

use colors::{brand, hsl, neutral};
use gpui::{px, App, Global, Hsla, Pixels, SharedString, Window, WindowAppearance};

use crate::colors::{danger, warning};

mod colors;
mod scale;

pub fn init(cx: &mut App) {
    Theme::sync_system_appearance(None, cx);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ThemeColor {
    // Surface colors
    pub background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface_background: Hsla,
    pub panel_background: Hsla,
    pub overlay: Hsla,
    pub title_bar: Hsla,
    pub title_bar_border: Hsla,
    pub window_border: Hsla,

    // Border colors
    pub border: Hsla,
    pub border_variant: Hsla,
    pub border_focused: Hsla,
    pub border_selected: Hsla,
    pub border_transparent: Hsla,
    pub border_disabled: Hsla,
    pub ring: Hsla,

    // Text colors
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_accent: Hsla,

    // Icon colors
    pub icon: Hsla,
    pub icon_muted: Hsla,
    pub icon_accent: Hsla,

    // Element colors
    pub element_foreground: Hsla,
    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    pub element_disabled: Hsla,

    // Secondary element colors
    pub secondary_foreground: Hsla,
    pub secondary_background: Hsla,
    pub secondary_hover: Hsla,
    pub secondary_active: Hsla,
    pub secondary_selected: Hsla,
    pub secondary_disabled: Hsla,

    // Danger element colors
    pub danger_foreground: Hsla,
    pub danger_background: Hsla,
    pub danger_hover: Hsla,
    pub danger_active: Hsla,
    pub danger_selected: Hsla,
    pub danger_disabled: Hsla,

    // Warning element colors
    pub warning_foreground: Hsla,
    pub warning_background: Hsla,
    pub warning_hover: Hsla,
    pub warning_active: Hsla,
    pub warning_selected: Hsla,
    pub warning_disabled: Hsla,

    // Ghost element colors
    pub ghost_element_background: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_active: Hsla,
    pub ghost_element_selected: Hsla,
    pub ghost_element_disabled: Hsla,

    // Tab colors
    pub tab_inactive_background: Hsla,
    pub tab_hover_background: Hsla,
    pub tab_active_background: Hsla,

    // Scrollbar colors
    pub scrollbar_thumb_background: Hsla,
    pub scrollbar_thumb_hover_background: Hsla,
    pub scrollbar_thumb_border: Hsla,
    pub scrollbar_track_background: Hsla,
    pub scrollbar_track_border: Hsla,

    // Interactive colors
    pub drop_target_background: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
}

/// The default colors for the theme.
///
/// Themes that do not specify all colors are refined off of these defaults.
impl ThemeColor {
    /// Returns the default colors for light themes.
    ///
    /// Themes that do not specify all colors are refined off of these defaults.
    pub fn light() -> Self {
        Self {
            background: neutral().light().step_1(),
            surface_background: neutral().light().step_2(),
            elevated_surface_background: neutral().light().step_3(),
            panel_background: gpui::white(),
            overlay: neutral().light_alpha().step_3(),
            title_bar: gpui::transparent_black(),
            title_bar_border: gpui::transparent_black(),
            window_border: hsl(240.0, 5.9, 78.0),

            border: neutral().light().step_6(),
            border_variant: neutral().light().step_5(),
            border_focused: brand().light().step_7(),
            border_selected: brand().light().step_7(),
            border_transparent: gpui::transparent_black(),
            border_disabled: neutral().light().step_3(),
            ring: brand().light().step_8(),

            text: neutral().light().step_12(),
            text_muted: neutral().light().step_11(),
            text_placeholder: neutral().light().step_10(),
            text_accent: brand().light().step_11(),

            icon: neutral().light().step_11(),
            icon_muted: neutral().light().step_10(),
            icon_accent: brand().light().step_11(),

            element_foreground: brand().light().step_12(),
            element_background: brand().light().step_9(),
            element_hover: brand().light_alpha().step_10(),
            element_active: brand().light().step_10(),
            element_selected: brand().light().step_11(),
            element_disabled: brand().light_alpha().step_3(),

            secondary_foreground: brand().light().step_12(),
            secondary_background: brand().light().step_3(),
            secondary_hover: brand().light_alpha().step_4(),
            secondary_active: brand().light().step_5(),
            secondary_selected: brand().light().step_5(),
            secondary_disabled: brand().light_alpha().step_3(),

            danger_foreground: danger().light().step_12(),
            danger_background: danger().light().step_9(),
            danger_hover: danger().light_alpha().step_10(),
            danger_active: danger().light().step_10(),
            danger_selected: danger().light().step_11(),
            danger_disabled: danger().light_alpha().step_3(),

            warning_foreground: warning().light().step_12(),
            warning_background: warning().light().step_9(),
            warning_hover: warning().light_alpha().step_10(),
            warning_active: warning().light().step_10(),
            warning_selected: warning().light().step_11(),
            warning_disabled: warning().light_alpha().step_3(),

            ghost_element_background: gpui::transparent_black(),
            ghost_element_hover: neutral().light_alpha().step_3(),
            ghost_element_active: neutral().light_alpha().step_4(),
            ghost_element_selected: neutral().light_alpha().step_5(),
            ghost_element_disabled: neutral().light_alpha().step_2(),

            tab_inactive_background: neutral().light().step_3(),
            tab_hover_background: neutral().light().step_4(),
            tab_active_background: neutral().light().step_5(),

            scrollbar_thumb_background: neutral().light_alpha().step_3(),
            scrollbar_thumb_hover_background: neutral().light_alpha().step_4(),
            scrollbar_thumb_border: gpui::transparent_black(),
            scrollbar_track_background: gpui::transparent_black(),
            scrollbar_track_border: neutral().light().step_5(),

            drop_target_background: brand().light_alpha().step_2(),
            cursor: hsl(200., 100., 50.),
            selection: hsl(200., 100., 50.).opacity(5.),
        }
    }

    /// Returns the default colors for dark themes.
    ///
    /// Themes that do not specify all colors are refined off of these defaults.
    pub fn dark() -> Self {
        Self {
            background: neutral().dark().step_1(),
            surface_background: neutral().dark().step_2(),
            elevated_surface_background: neutral().dark().step_3(),
            panel_background: gpui::black(),
            overlay: neutral().dark_alpha().step_3(),
            title_bar: gpui::transparent_black(),
            title_bar_border: gpui::transparent_black(),
            window_border: hsl(240.0, 3.7, 28.0),

            border: neutral().dark().step_6(),
            border_variant: neutral().dark().step_5(),
            border_focused: brand().dark().step_7(),
            border_selected: brand().dark().step_7(),
            border_transparent: gpui::transparent_black(),
            border_disabled: neutral().dark().step_3(),
            ring: brand().dark().step_8(),

            text: neutral().dark().step_12(),
            text_muted: neutral().dark().step_11(),
            text_placeholder: neutral().dark().step_10(),
            text_accent: brand().dark().step_11(),

            icon: neutral().dark().step_11(),
            icon_muted: neutral().dark().step_10(),
            icon_accent: brand().dark().step_11(),

            element_foreground: brand().dark().step_1(),
            element_background: brand().dark().step_9(),
            element_hover: brand().dark_alpha().step_10(),
            element_active: brand().dark().step_10(),
            element_selected: brand().dark().step_11(),
            element_disabled: brand().dark_alpha().step_3(),

            secondary_foreground: brand().dark().step_12(),
            secondary_background: brand().dark().step_3(),
            secondary_hover: brand().dark_alpha().step_4(),
            secondary_active: brand().dark().step_5(),
            secondary_selected: brand().dark().step_5(),
            secondary_disabled: brand().dark_alpha().step_3(),

            danger_foreground: danger().dark().step_12(),
            danger_background: danger().dark().step_9(),
            danger_hover: danger().dark_alpha().step_10(),
            danger_active: danger().dark().step_10(),
            danger_selected: danger().dark().step_11(),
            danger_disabled: danger().dark_alpha().step_3(),

            warning_foreground: warning().dark().step_12(),
            warning_background: warning().dark().step_9(),
            warning_hover: warning().dark_alpha().step_10(),
            warning_active: warning().dark().step_10(),
            warning_selected: warning().dark().step_11(),
            warning_disabled: warning().dark_alpha().step_3(),

            ghost_element_background: gpui::transparent_black(),
            ghost_element_hover: neutral().dark_alpha().step_3(),
            ghost_element_active: neutral().dark_alpha().step_4(),
            ghost_element_selected: neutral().dark_alpha().step_5(),
            ghost_element_disabled: neutral().dark_alpha().step_2(),

            tab_inactive_background: neutral().dark().step_3(),
            tab_hover_background: neutral().dark().step_4(),
            tab_active_background: neutral().dark().step_5(),

            scrollbar_thumb_background: neutral().dark_alpha().step_3(),
            scrollbar_thumb_hover_background: neutral().dark_alpha().step_4(),
            scrollbar_thumb_border: gpui::transparent_black(),
            scrollbar_track_background: gpui::transparent_black(),
            scrollbar_track_border: neutral().dark().step_5(),

            drop_target_background: brand().dark_alpha().step_2(),
            cursor: hsl(200., 100., 50.),
            selection: hsl(200., 100., 50.).opacity(5.),
        }
    }
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

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq, Hash)]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
}

impl ThemeMode {
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }

    /// Return lower_case theme name: `light`, `dark`.
    pub fn name(&self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

impl From<WindowAppearance> for ThemeMode {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub colors: ThemeColor,
    pub mode: ThemeMode,
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub radius: Pixels,
}

impl Deref for Theme {
    type Target = ThemeColor;

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

    /// Change the app's appearance
    pub fn change(mode: impl Into<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
        let mode = mode.into();
        let colors = match mode {
            ThemeMode::Light => ThemeColor::light(),
            ThemeMode::Dark => ThemeColor::dark(),
        };

        if !cx.has_global::<Theme>() {
            let theme = Theme::from(colors);
            cx.set_global(theme);
        }

        let theme = cx.global_mut::<Theme>();

        theme.mode = mode;
        theme.colors = colors;

        if let Some(window) = window {
            window.refresh();
        }
    }
}

impl From<ThemeColor> for Theme {
    fn from(colors: ThemeColor) -> Self {
        let mode = ThemeMode::default();

        Theme {
            font_size: px(15.),
            font_family: if cfg!(target_os = "macos") {
                ".SystemUIFont".into()
            } else if cfg!(target_os = "windows") {
                "Segoe UI".into()
            } else {
                "FreeMono".into()
            },
            radius: px(5.),
            mode,
            colors,
        }
    }
}
