use std::path::Path;

use gpui::{SharedString, WindowAppearance};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ThemeColors;

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

/// Theme family
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
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

    /// Load a theme family from a JSON file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the JSON file containing the theme family. This can be
    ///   an absolute path or a path relative to the current working directory.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ThemeFamily)` if the file was successfully loaded and parsed,
    /// or `Err(anyhow::Error)` if there was an error reading or parsing the file.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be read (permission issues, file doesn't exist, etc.)
    /// - The file contains invalid JSON
    /// - The JSON structure doesn't match the `ThemeFamily` schema
    ///
    /// # Example
    ///
    /// ```no_run
    /// use theme::ThemeFamily;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// // Load from a relative path
    /// let theme = ThemeFamily::from_file("assets/themes/my-theme.json")?;
    ///
    /// // Load from an absolute path
    /// let theme = ThemeFamily::from_file("/path/to/themes/my-theme.json")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let json_data = std::fs::read(path)?;
        let theme_family = serde_json::from_slice(&json_data)?;

        Ok(theme_family)
    }

    /// Load a theme family from a JSON file in the assets/themes directory.
    ///
    /// This function looks for the file at `assets/themes/{name}.json` relative
    /// to the current working directory. This is useful for loading themes
    /// from the standard theme directory in the project structure.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the theme file (without the .json extension)
    ///
    /// # Returns
    ///
    /// Returns `Ok(ThemeFamily)` if the file was successfully loaded and parsed,
    /// or `Err(anyhow::Error)` if there was an error reading or parsing the file.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be read (permission issues, file doesn't exist, etc.)
    /// - The file contains invalid JSON
    /// - The JSON structure doesn't match the `ThemeFamily` schema
    ///
    /// # Example
    ///
    /// ```no_run
    /// use theme::ThemeFamily;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// // Assuming the file exists at `assets/themes/my-theme.json`
    /// let theme = ThemeFamily::from_assets("my-theme")?;
    ///
    /// println!("Loaded theme: {}", theme.name);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_assets(name: &str) -> anyhow::Result<Self> {
        let path = format!("assets/themes/{}.json", name);
        Self::from_file(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_from_file() {
        // Create a temporary directory for our test
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test-theme.json");

        // Create a minimal valid theme JSON with hex colors
        // Using simple hex colors that Hsla can parse
        // Note: We need to escape the # characters in the raw string
        let json_data = r##"{
            "id": "test-theme",
            "name": "Test Theme",
            "light": {
                "background": "#ffffff",
                "surface_background": "#fafafa",
                "elevated_surface_background": "#f5f5f5",
                "panel_background": "#ffffff",
                "overlay": "#0000001a",
                "title_bar": "#00000000",
                "title_bar_inactive": "#ffffff",
                "window_border": "#c7c7cf",
                "border": "#dbdbdb",
                "border_variant": "#d1d1d1",
                "border_focused": "#3366cc",
                "border_selected": "#3366cc",
                "border_transparent": "#00000000",
                "border_disabled": "#e6e6e6",
                "ring": "#4d79d6",
                "text": "#1a1a1a",
                "text_muted": "#4d4d4d",
                "text_placeholder": "#808080",
                "text_accent": "#3366cc",
                "icon": "#4d4d4d",
                "icon_muted": "#808080",
                "icon_accent": "#3366cc",
                "element_foreground": "#ffffff",
                "element_background": "#3366cc",
                "element_hover": "#3366cce6",
                "element_active": "#2e5cb8",
                "element_selected": "#2952a3",
                "element_disabled": "#3366cc4d",
                "secondary_foreground": "#2952a3",
                "secondary_background": "#e6ecf5",
                "secondary_hover": "#3366cc1a",
                "secondary_active": "#d9e2f0",
                "secondary_selected": "#d9e2f0",
                "secondary_disabled": "#3366cc4d",
                "danger_foreground": "#ffffff",
                "danger_background": "#f5e6e6",
                "danger_hover": "#cc33331a",
                "danger_active": "#f0d9d9",
                "danger_selected": "#f0d9d9",
                "danger_disabled": "#cc33334d",
                "warning_foreground": "#1a1a1a",
                "warning_background": "#f5f0e6",
                "warning_hover": "#cc99331a",
                "warning_active": "#f0ead9",
                "warning_selected": "#f0ead9",
                "warning_disabled": "#cc99334d",
                "ghost_element_background": "#00000000",
                "ghost_element_background_alt": "#e6e6e6",
                "ghost_element_hover": "#0000001a",
                "ghost_element_active": "#d9d9d9",
                "ghost_element_selected": "#d9d9d9",
                "ghost_element_disabled": "#0000000d",
                "tab_inactive_background": "#e6e6e6",
                "tab_hover_background": "#e0e0e0",
                "tab_active_background": "#d9d9d9",
                "scrollbar_thumb_background": "#00000033",
                "scrollbar_thumb_hover_background": "#0000004d",
                "scrollbar_thumb_border": "#00000000",
                "scrollbar_track_background": "#00000000",
                "scrollbar_track_border": "#d9d9d9",
                "drop_target_background": "#3366cc1a",
                "cursor": "#3399ff",
                "selection": "#3399ff40"
            },
            "dark": {
                "background": "#1a1a1a",
                "surface_background": "#1f1f1f",
                "elevated_surface_background": "#242424",
                "panel_background": "#262626",
                "overlay": "#ffffff1a",
                "title_bar": "#00000000",
                "title_bar_inactive": "#1a1a1a",
                "window_border": "#404046",
                "border": "#404040",
                "border_variant": "#383838",
                "border_focused": "#4d79d6",
                "border_selected": "#4d79d6",
                "border_transparent": "#00000000",
                "border_disabled": "#2e2e2e",
                "ring": "#668cdf",
                "text": "#f2f2f2",
                "text_muted": "#b3b3b3",
                "text_placeholder": "#808080",
                "text_accent": "#668cdf",
                "icon": "#b3b3b3",
                "icon_muted": "#808080",
                "icon_accent": "#668cdf",
                "element_foreground": "#ffffff",
                "element_background": "#4d79d6",
                "element_hover": "#4d79d6e6",
                "element_active": "#456dc1",
                "element_selected": "#3e62ac",
                "element_disabled": "#4d79d64d",
                "secondary_foreground": "#3e62ac",
                "secondary_background": "#2a3652",
                "secondary_hover": "#4d79d61a",
                "secondary_active": "#303d5c",
                "secondary_selected": "#303d5c",
                "secondary_disabled": "#4d79d64d",
                "danger_foreground": "#ffffff",
                "danger_background": "#522a2a",
                "danger_hover": "#d64d4d1a",
                "danger_active": "#5c3030",
                "danger_selected": "#5c3030",
                "danger_disabled": "#d64d4d4d",
                "warning_foreground": "#f2f2f2",
                "warning_background": "#52482a",
                "warning_hover": "#d6b34d1a",
                "warning_active": "#5c5430",
                "warning_selected": "#5c5430",
                "warning_disabled": "#d6b34d4d",
                "ghost_element_background": "#00000000",
                "ghost_element_background_alt": "#2e2e2e",
                "ghost_element_hover": "#ffffff1a",
                "ghost_element_active": "#383838",
                "ghost_element_selected": "#383838",
                "ghost_element_disabled": "#ffffff0d",
                "tab_inactive_background": "#2e2e2e",
                "tab_hover_background": "#333333",
                "tab_active_background": "#383838",
                "scrollbar_thumb_background": "#ffffff33",
                "scrollbar_thumb_hover_background": "#ffffff4d",
                "scrollbar_thumb_border": "#00000000",
                "scrollbar_track_background": "#00000000",
                "scrollbar_track_border": "#383838",
                "drop_target_background": "#4d79d61a",
                "cursor": "#4db3ff",
                "selection": "#4db3ff40"
            }
        }"##;

        // Write the JSON to the file
        fs::write(&file_path, json_data).unwrap();

        // Test loading the theme from file
        let theme = ThemeFamily::from_file(&file_path).unwrap();

        // Verify the loaded theme
        assert_eq!(theme.id, "test-theme");
        assert_eq!(theme.name, "Test Theme");

        // Clean up
        dir.close().unwrap();
    }

    #[test]
    fn test_from_file_nonexistent() {
        // Test that loading a non-existent file returns an error
        let result = ThemeFamily::from_file("non-existent-file.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_file_invalid_json() {
        // Create a temporary directory for our test
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("invalid-theme.json");

        // Write invalid JSON
        fs::write(&file_path, "invalid json").unwrap();

        // Test that loading invalid JSON returns an error
        let result = ThemeFamily::from_file(&file_path);
        assert!(result.is_err());

        // Clean up
        dir.close().unwrap();
    }
}
