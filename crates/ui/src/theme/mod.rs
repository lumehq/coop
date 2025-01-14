use crate::scroll::ScrollbarShow;
use colors::{default_color_scales, hsl};
use gpui::{
    blue, hsla, transparent_black, AppContext, Global, Hsla, ModelContext, SharedString,
    ViewContext, WindowAppearance, WindowContext,
};
use scale::ColorScaleSet;
use std::ops::{Deref, DerefMut};

pub mod colors;
pub mod scale;

#[derive(Debug, Clone, Copy, Default)]
pub struct ThemeColors {
    pub background: Hsla,
    pub border: Hsla,
    pub window_border: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub card: Hsla,
    pub card_foreground: Hsla,
    pub danger: Hsla,
    pub danger_active: Hsla,
    pub danger_foreground: Hsla,
    pub danger_hover: Hsla,
    pub drag_border: Hsla,
    pub drop_target: Hsla,
    pub foreground: Hsla,
    pub input: Hsla,
    pub link: Hsla,
    pub link_active: Hsla,
    pub link_hover: Hsla,
    pub list: Hsla,
    pub list_active: Hsla,
    pub list_active_border: Hsla,
    pub list_even: Hsla,
    pub list_head: Hsla,
    pub list_hover: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub popover: Hsla,
    pub popover_foreground: Hsla,
    pub primary: Hsla,
    pub primary_active: Hsla,
    pub primary_foreground: Hsla,
    pub primary_hover: Hsla,
    pub progress_bar: Hsla,
    pub ring: Hsla,
    pub scrollbar: Hsla,
    pub scrollbar_thumb: Hsla,
    pub scrollbar_thumb_hover: Hsla,
    pub secondary: Hsla,
    pub secondary_active: Hsla,
    pub secondary_foreground: Hsla,
    pub secondary_hover: Hsla,
    pub selection: Hsla,
    pub skeleton: Hsla,
    pub slider_bar: Hsla,
    pub slider_thumb: Hsla,
    pub tab: Hsla,
    pub tab_active: Hsla,
    pub tab_active_foreground: Hsla,
    pub tab_bar: Hsla,
    pub tab_foreground: Hsla,
    pub title_bar: Hsla,
    pub title_bar_border: Hsla,
}

impl ThemeColors {
    pub fn light() -> Self {
        Self {
            background: hsl(0.0, 0.0, 100.),
            accent: hsl(240.0, 5.0, 96.0),
            accent_foreground: hsl(240.0, 5.9, 10.0),
            border: hsl(240.0, 5.9, 90.0),
            window_border: hsl(240.0, 5.9, 78.0),
            card: hsl(0.0, 0.0, 100.0),
            card_foreground: hsl(240.0, 10.0, 3.9),
            danger: hsl(0.0, 84.2, 60.2),
            danger_active: hsl(0.0, 84.2, 47.0),
            danger_foreground: hsl(0.0, 0.0, 98.0),
            danger_hover: hsl(0.0, 84.2, 65.0),
            drag_border: blue(),
            drop_target: hsl(235.0, 30., 44.0).opacity(0.25),
            foreground: hsl(240.0, 10., 3.9),
            input: hsl(240.0, 5.9, 90.0),
            link: hsl(221.0, 83.0, 53.0),
            link_active: hsl(221.0, 83.0, 53.0).darken(0.2),
            link_hover: hsl(221.0, 83.0, 53.0).lighten(0.2),
            list: hsl(0.0, 0.0, 100.),
            list_active: hsl(211.0, 97.0, 85.0).opacity(0.2),
            list_active_border: hsl(211.0, 97.0, 85.0),
            list_even: hsl(240.0, 5.0, 96.0),
            list_head: hsl(0.0, 0.0, 100.),
            list_hover: hsl(240.0, 4.8, 95.0),
            muted: hsl(240.0, 4.8, 95.9),
            muted_foreground: hsl(240.0, 3.8, 46.1),
            popover: hsl(0.0, 0.0, 100.0),
            popover_foreground: hsl(240.0, 10.0, 3.9),
            primary: hsl(223.0, 5.9, 10.0),
            primary_active: hsl(223.0, 1.9, 25.0),
            primary_foreground: hsl(223.0, 0.0, 98.0),
            primary_hover: hsl(223.0, 5.9, 15.0),
            progress_bar: hsl(223.0, 5.9, 10.0),
            ring: hsl(240.0, 5.9, 65.0),
            scrollbar: hsl(0., 0., 97.).opacity(0.75),
            scrollbar_thumb: hsl(0., 0., 69.).opacity(0.9),
            scrollbar_thumb_hover: hsl(0., 0., 59.),
            secondary: hsl(240.0, 5.9, 96.9),
            secondary_active: hsl(240.0, 5.9, 90.),
            secondary_foreground: hsl(240.0, 59.0, 10.),
            secondary_hover: hsl(240.0, 5.9, 98.),
            selection: hsl(211.0, 97.0, 85.0),
            skeleton: hsl(223.0, 5.9, 10.0).opacity(0.1),
            slider_bar: hsl(223.0, 5.9, 10.0),
            slider_thumb: hsl(0.0, 0.0, 100.0),
            tab: transparent_black(),
            tab_active: hsl(0.0, 0.0, 100.0),
            tab_active_foreground: hsl(240.0, 10., 3.9),
            tab_bar: hsl(240.0, 4.8, 95.9),
            tab_foreground: hsl(240.0, 10., 3.9),
            title_bar: hsl(0.0, 0.0, 98.0),
            title_bar_border: hsl(220.0, 13.0, 91.0),
        }
    }

    pub fn dark() -> Self {
        Self {
            background: hsl(0.0, 0.0, 8.0),
            accent: hsl(240.0, 3.7, 15.9),
            accent_foreground: hsl(0.0, 0.0, 78.0),
            border: hsl(240.0, 3.7, 16.9),
            window_border: hsl(240.0, 3.7, 28.0),
            card: hsl(0.0, 0.0, 8.0),
            card_foreground: hsl(0.0, 0.0, 78.0),
            danger: hsl(0.0, 62.8, 30.6),
            danger_active: hsl(0.0, 62.8, 20.6),
            danger_foreground: hsl(0.0, 0.0, 78.0),
            danger_hover: hsl(0.0, 62.8, 35.6),
            drag_border: blue(),
            drop_target: hsl(235.0, 30., 44.0).opacity(0.1),
            foreground: hsl(0., 0., 78.),
            input: hsl(240.0, 3.7, 15.9),
            link: hsl(221.0, 83.0, 53.0),
            link_active: hsl(221.0, 83.0, 53.0).darken(0.2),
            link_hover: hsl(221.0, 83.0, 53.0).lighten(0.2),
            list: hsl(0.0, 0.0, 8.0),
            list_active: hsl(240.0, 3.7, 15.0).opacity(0.2),
            list_active_border: hsl(240.0, 5.9, 35.5),
            list_even: hsl(240.0, 3.7, 10.0),
            list_head: hsl(0.0, 0.0, 8.0),
            list_hover: hsl(240.0, 3.7, 15.9),
            muted: hsl(240.0, 3.7, 15.9),
            muted_foreground: hsl(240.0, 5.0, 64.9),
            popover: hsl(0.0, 0.0, 10.),
            popover_foreground: hsl(0.0, 0.0, 78.0),
            primary: hsl(223.0, 0.0, 98.0),
            primary_active: hsl(223.0, 0.0, 80.0),
            primary_foreground: hsl(223.0, 5.9, 10.0),
            primary_hover: hsl(223.0, 0.0, 90.0),
            progress_bar: hsl(223.0, 0.0, 98.0),
            ring: hsl(240.0, 4.9, 83.9),
            scrollbar: hsl(240., 1., 15.).opacity(0.75),
            scrollbar_thumb: hsl(0., 0., 48.).opacity(0.9),
            scrollbar_thumb_hover: hsl(0., 0., 68.),
            secondary: hsl(240.0, 0., 13.0),
            secondary_active: hsl(240.0, 0., 10.),
            secondary_foreground: hsl(0.0, 0.0, 78.0),
            secondary_hover: hsl(240.0, 0., 15.),
            selection: hsl(211.0, 97.0, 22.0),
            skeleton: hsla(223.0, 0.0, 98.0, 0.1),
            slider_bar: hsl(223.0, 0.0, 98.0),
            slider_thumb: hsl(0.0, 0.0, 8.0),
            tab: transparent_black(),
            tab_active: hsl(0.0, 0.0, 8.0),
            tab_active_foreground: hsl(0., 0., 78.),
            tab_bar: hsl(299.0, 0., 5.5),
            tab_foreground: hsl(0., 0., 78.),
            title_bar: hsl(240.0, 0.0, 10.0),
            title_bar_border: hsl(240.0, 3.7, 15.9),
        }
    }
}

pub trait Colorize {
    fn opacity(&self, opacity: f32) -> Hsla;
    fn divide(&self, divisor: f32) -> Hsla;
    fn invert(&self) -> Hsla;
    fn invert_l(&self) -> Hsla;
    fn lighten(&self, amount: f32) -> Hsla;
    fn darken(&self, amount: f32) -> Hsla;
    fn apply(&self, base_color: Hsla) -> Hsla;
}

impl Colorize for Hsla {
    /// Returns a new color with the given opacity.
    ///
    /// The opacity is a value between 0.0 and 1.0, where 0.0 is fully transparent and 1.0 is fully opaque.
    fn opacity(&self, factor: f32) -> Hsla {
        Hsla {
            a: self.a * factor.clamp(0.0, 1.0),
            ..*self
        }
    }

    /// Returns a new color with each channel divided by the given divisor.
    ///
    /// The divisor in range of 0.0 .. 1.0
    fn divide(&self, divisor: f32) -> Hsla {
        Hsla {
            a: divisor,
            ..*self
        }
    }

    /// Return inverted color
    fn invert(&self) -> Hsla {
        Hsla {
            h: (self.h + 1.8) % 3.6,
            s: 1.0 - self.s,
            l: 1.0 - self.l,
            a: self.a,
        }
    }

    /// Return inverted lightness
    fn invert_l(&self) -> Hsla {
        Hsla {
            l: 1.0 - self.l,
            ..*self
        }
    }

    /// Return a new color with the lightness increased by the given factor.
    ///
    /// factor range: 0.0 .. 1.0
    fn lighten(&self, factor: f32) -> Hsla {
        let l = self.l * (1.0 + factor.clamp(0.0, 1.0));

        Hsla { l, ..*self }
    }

    /// Return a new color with the darkness increased by the given factor.
    ///
    /// factor range: 0.0 .. 1.0
    fn darken(&self, factor: f32) -> Hsla {
        let l = self.l * (1.0 - factor.clamp(0.0, 1.0));

        Hsla { l, ..*self }
    }

    /// Return a new color with the same lightness and alpha but different hue and saturation.
    fn apply(&self, new_color: Hsla) -> Hsla {
        Hsla {
            h: new_color.h,
            s: new_color.s,
            l: self.l,
            a: self.a,
        }
    }
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for AppContext {
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

impl<V> ActiveTheme for ViewContext<'_, V> {
    fn theme(&self) -> &Theme {
        self.deref().theme()
    }
}

impl<V> ActiveTheme for ModelContext<'_, V> {
    fn theme(&self) -> &Theme {
        self.deref().theme()
    }
}

impl ActiveTheme for WindowContext<'_> {
    fn theme(&self) -> &Theme {
        self.deref().theme()
    }
}

pub fn init(cx: &mut AppContext) {
    Theme::sync_system_appearance(cx)
}

pub struct Theme {
    pub colors: ThemeColors,
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
    pub transparent: Hsla,
    /// Show the scrollbar mode, default: Scrolling
    pub scrollbar_show: ScrollbarShow,
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
    pub fn global(cx: &AppContext) -> &Theme {
        cx.global::<Theme>()
    }

    /// Returns the global theme mutable reference
    pub fn global_mut(cx: &mut AppContext) -> &mut Theme {
        cx.global_mut::<Theme>()
    }

    /// Sync the theme with the system appearance
    pub fn sync_system_appearance(cx: &mut AppContext) {
        match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                Self::change(Appearance::Dark, cx)
            }
            WindowAppearance::Light | WindowAppearance::VibrantLight => {
                Self::change(Appearance::Light, cx)
            }
        }
    }

    pub fn change(mode: Appearance, cx: &mut AppContext) {
        let colors = match mode {
            Appearance::Light => ThemeColors::light(),
            Appearance::Dark => ThemeColors::dark(),
        };

        let mut theme = Theme::from(colors);
        theme.appearance = mode;

        cx.set_global(theme);
        cx.refresh();
    }
}

impl From<ThemeColors> for Theme {
    fn from(colors: ThemeColors) -> Self {
        let color_scales = default_color_scales();

        Theme {
            base: color_scales.gray,
            accent: color_scales.yellow,
            appearance: Appearance::default(),
            transparent: Hsla::transparent_black(),
            font_size: 16.0,
            font_family: if cfg!(target_os = "macos") {
                ".SystemUIFont".into()
            } else if cfg!(target_os = "windows") {
                "Segoe UI".into()
            } else {
                "FreeMono".into()
            },
            radius: 6.0,
            shadow: false,
            scrollbar_show: ScrollbarShow::default(),
            colors,
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
