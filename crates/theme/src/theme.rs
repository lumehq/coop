use gpui::{SharedString, WindowAppearance};

use crate::ThemeColors;

/// Theme family
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeFamily {
    /// The unique identifier for the theme.
    pub id: String,

    /// The name of the theme.
    pub name: SharedString,

    /// The light colors for the theme.
    pub light: ThemeColors,

    /// The dark colors for the theme.
    pub dark: ThemeColors,
}

impl Default for ThemeFamily {
    fn default() -> Self {
        ThemeFamily {
            id: "coop".into(),
            name: "Coop Default Theme".into(),
            light: ThemeColors::light(),
            dark: ThemeColors::dark(),
        }
    }
}

impl ThemeFamily {
    /// Returns the light colors for the theme.
    #[inline(always)]
    pub fn light(&self) -> &ThemeColors {
        &self.light
    }

    /// Returns the dark colors for the theme.
    #[inline(always)]
    pub fn dark(&self) -> &ThemeColors {
        &self.dark
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq, Hash)]
pub enum ThemeMode {
    #[default]
    Light,
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
