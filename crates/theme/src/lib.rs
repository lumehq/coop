use std::ops::{Deref, DerefMut};

use colors::{brand, hsl, neutral};
use gpui::{black, px, white, App, Global, Hsla, Pixels, SharedString, Window, WindowAppearance};

mod colors;
mod scale;

pub fn init(cx: &mut App) {
    Theme::sync_system_appearance(None, cx);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ThemeColor {
    /// Border color. Used for most borders, is usually a high contrast color.
    pub border: Hsla,
    /// Border color. Used for deemphasized borders, like a visual divider between two sections
    pub border_variant: Hsla,
    /// Border color. Used for focused elements, like keyboard focused list item.
    pub border_focused: Hsla,
    /// Border color. Used for selected elements, like an active search filter or selected checkbox.
    pub border_selected: Hsla,
    /// Border color. Used for transparent borders. Used for placeholder borders when an element gains a border on state change.
    pub border_transparent: Hsla,
    /// Border color. Used for disabled elements, like a disabled input or button.
    pub border_disabled: Hsla,
    /// Background color. Used for elevated surfaces, like a context menu, popup, or dialog.
    pub elevated_surface_background: Hsla,
    /// Background color. Used for grounded surfaces like a panel or tab.
    pub surface_background: Hsla,
    /// Background color. Used for the app background and blank panels or windows.
    pub background: Hsla,
    /// Text color. Used for the foreground of an element.
    pub element_foreground: Hsla,
    /// Background color. Used for the background of an element that should have a different background than the surface it's on.
    ///
    /// Elements might include: Buttons, Inputs, Checkboxes, Radio Buttons...
    ///
    /// For an element that should have the same background as the surface it's on, use `ghost_element_background`.
    pub element_background: Hsla,
    /// Background color. Used for the hover state of an element that should have a different background than the surface it's on.
    ///
    /// Hover states are triggered by the mouse entering an element, or a finger touching an element on a touch screen.
    pub element_hover: Hsla,
    /// Background color. Used for the active state of an element that should have a different background than the surface it's on.
    ///
    /// Active states are triggered by the mouse button being pressed down on an element, or the Return button or other activator being pressed.
    pub element_active: Hsla,
    /// Background color. Used for the selected state of an element that should have a different background than the surface it's on.
    ///
    /// Selected states are triggered by the element being selected (or "activated") by the user.
    ///
    /// This could include a selected checkbox, a toggleable button that is toggled on, etc.
    pub element_selected: Hsla,
    /// Background Color. Used for the disabled state of a element that should have a different background than the surface it's on.
    ///
    /// Disabled states are shown when a user cannot interact with an element, like a disabled button or input.
    pub element_disabled: Hsla,
    /// Background color. Used for the area that shows where a dragged element will be dropped.
    pub drop_target_background: Hsla,
    /// Used for the background of a ghost element that should have the same background as the surface it's on.
    ///
    /// Elements might include: Buttons, Inputs, Checkboxes, Radio Buttons...
    ///
    /// For an element that should have a different background than the surface it's on, use `element_background`.
    pub ghost_element_background: Hsla,
    /// Background Color. Used for the hover state of a ghost element that should have the same background as the surface it's on.
    ///
    /// Hover states are triggered by the mouse entering an element, or a finger touching an element on a touch screen.
    pub ghost_element_hover: Hsla,
    /// Background Color. Used for the active state of a ghost element that should have the same background as the surface it's on.
    ///
    /// Active states are triggered by the mouse button being pressed down on an element, or the Return button or other activator being pressed.
    pub ghost_element_active: Hsla,
    /// Background Color. Used for the selected state of a ghost element that should have the same background as the surface it's on.
    ///
    /// Selected states are triggered by the element being selected (or "activated") by the user.
    ///
    /// This could include a selected checkbox, a toggleable button that is toggled on, etc.
    pub ghost_element_selected: Hsla,
    /// Background Color. Used for the disabled state of a ghost element that should have the same background as the surface it's on.
    ///
    /// Disabled states are shown when a user cannot interact with an element, like a disabled button or input.
    pub ghost_element_disabled: Hsla,
    /// Text color. Default text color used for most text.
    pub text: Hsla,
    /// Text color. Color of muted or deemphasized text. It is a subdued version of the standard text color.
    pub text_muted: Hsla,
    /// Text color. Color of the placeholder text typically shown in input fields to guide the user to enter valid data.
    pub text_placeholder: Hsla,
    /// Text color. Color used for emphasis or highlighting certain text, like an active filter or a matched character in a search.
    pub text_accent: Hsla,
    /// Fill color. Used for the default fill color of an icon.
    pub icon: Hsla,
    /// Fill color. Used for the muted or deemphasized fill color of an icon.
    ///
    /// This might be used to show an icon in an inactive pane, or to deemphasize a series of icons to give them less visual weight.
    pub icon_muted: Hsla,
    /// Fill color. Used for the accent fill color of an icon.
    ///
    /// This might be used to show when a toggleable icon button is selected.
    pub icon_accent: Hsla,
    /// The color of the scrollbar thumb.
    pub scrollbar_thumb_background: Hsla,
    /// The color of the scrollbar thumb when hovered over.
    pub scrollbar_thumb_hover_background: Hsla,
    /// The border color of the scrollbar thumb.
    pub scrollbar_thumb_border: Hsla,
    /// The background color of the scrollbar track.
    pub scrollbar_track_background: Hsla,
    /// The border color of the scrollbar track.
    pub scrollbar_track_border: Hsla,
    /// Background color. Used for the background of a panel
    pub panel_background: Hsla,
    /// Border color. Used for outline border.
    pub ring: Hsla,
    /// Background color. Used for inactive tab.
    pub tab_inactive_background: Hsla,
    /// Background color. Used for hovered tab.
    pub tab_hover_background: Hsla,
    /// Background color. Used for active tab.
    pub tab_active_background: Hsla,
    /// Background color. Used for Title Bar.
    pub title_bar: Hsla,
    /// Border color. Used for Title Bar.
    pub title_bar_border: Hsla,
    /// Background color. Used for modal's overlay.
    pub overlay: Hsla,
    /// Window border color.
    ///
    /// # Platform specific:
    ///
    /// This is only works on Linux, other platforms we can't change the window border color.
    pub window_border: Hsla,
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
            border: neutral().light().step_6(),
            border_variant: neutral().light().step_5(),
            border_focused: brand().light().step_7(),
            border_selected: brand().light().step_7(),
            border_transparent: gpui::transparent_black(),
            border_disabled: neutral().light().step_3(),
            elevated_surface_background: neutral().light().step_3(),
            surface_background: neutral().light().step_2(),
            background: neutral().light().step_1(),
            element_foreground: brand().light().step_12(),
            element_background: brand().light().step_9(),
            element_hover: brand().light_alpha().step_10(),
            element_active: brand().light().step_10(),
            element_selected: brand().light().step_10(),
            element_disabled: brand().light_alpha().step_3(),
            drop_target_background: brand().light_alpha().step_2(),
            ghost_element_background: gpui::transparent_black(),
            ghost_element_hover: neutral().light_alpha().step_3(),
            ghost_element_active: neutral().light_alpha().step_4(),
            ghost_element_selected: neutral().light_alpha().step_5(),
            ghost_element_disabled: neutral().light_alpha().step_3(),
            text: neutral().light().step_12(),
            text_muted: neutral().light().step_11(),
            text_placeholder: neutral().light().step_10(),
            text_accent: brand().light().step_11(),
            icon: neutral().light().step_11(),
            icon_muted: neutral().light().step_10(),
            icon_accent: brand().light().step_11(),
            scrollbar_thumb_background: neutral().light_alpha().step_3(),
            scrollbar_thumb_hover_background: neutral().light_alpha().step_4(),
            scrollbar_thumb_border: gpui::transparent_black(),
            scrollbar_track_background: gpui::transparent_black(),
            scrollbar_track_border: neutral().light().step_5(),
            panel_background: white(),
            ring: brand().light().step_8(),
            tab_active_background: neutral().light().step_5(),
            tab_hover_background: neutral().light().step_4(),
            tab_inactive_background: neutral().light().step_3(),
            title_bar: gpui::transparent_black(),
            title_bar_border: gpui::transparent_black(),
            overlay: neutral().light_alpha().step_3(),
            window_border: hsl(240.0, 5.9, 78.0),
        }
    }

    /// Returns the default colors for dark themes.
    ///
    /// Themes that do not specify all colors are refined off of these defaults.
    pub fn dark() -> Self {
        Self {
            border: neutral().dark().step_6(),
            border_variant: neutral().dark().step_5(),
            border_focused: brand().dark().step_7(),
            border_selected: brand().dark().step_7(),
            border_transparent: gpui::transparent_black(),
            border_disabled: neutral().light().step_3(),
            elevated_surface_background: neutral().dark().step_3(),
            surface_background: neutral().dark().step_2(),
            background: neutral().dark().step_1(),
            element_foreground: brand().dark().step_12(),
            element_background: brand().dark().step_9(),
            element_hover: brand().dark_alpha().step_10(),
            element_active: brand().dark().step_10(),
            element_selected: brand().dark().step_10(),
            element_disabled: brand().dark_alpha().step_3(),
            drop_target_background: brand().dark_alpha().step_2(),
            ghost_element_background: gpui::transparent_black(),
            ghost_element_hover: neutral().dark_alpha().step_3(),
            ghost_element_active: neutral().dark_alpha().step_4(),
            ghost_element_selected: neutral().dark_alpha().step_5(),
            ghost_element_disabled: neutral().dark_alpha().step_3(),
            text: neutral().dark().step_12(),
            text_muted: neutral().dark().step_11(),
            text_placeholder: neutral().dark().step_10(),
            text_accent: brand().dark().step_11(),
            icon: neutral().dark().step_11(),
            icon_muted: neutral().dark().step_10(),
            icon_accent: brand().dark().step_11(),
            scrollbar_thumb_background: neutral().dark_alpha().step_3(),
            scrollbar_thumb_hover_background: neutral().dark_alpha().step_4(),
            scrollbar_thumb_border: gpui::transparent_black(),
            scrollbar_track_background: gpui::transparent_black(),
            scrollbar_track_border: neutral().dark().step_5(),
            panel_background: black(),
            ring: brand().dark().step_8(),
            tab_active_background: neutral().dark().step_5(),
            tab_hover_background: neutral().dark().step_4(),
            tab_inactive_background: neutral().dark().step_3(),
            title_bar: gpui::transparent_black(),
            title_bar_border: gpui::transparent_black(),
            overlay: neutral().dark_alpha().step_3(),
            window_border: hsl(240.0, 3.7, 28.0),
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
