use crate::{
    scroll::{Scrollable, ScrollbarAxis},
    theme::{scale::ColorScaleStep, ActiveTheme},
};
use gpui::{div, px, App, Axis, Div, Element, ElementId, EntityId, Pixels, Styled, Window};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

/// Returns a `Div` as horizontal flex layout.
pub fn h_flex() -> Div {
    div().h_flex()
}

/// Returns a `Div` as vertical flex layout.
pub fn v_flex() -> Div {
    div().v_flex()
}

macro_rules! font_weight {
    ($fn:ident, $const:ident) => {
        /// [docs](https://tailwindcss.com/docs/font-weight)
        fn $fn(self) -> Self {
            self.font_weight(gpui::FontWeight::$const)
        }
    };
}

/// Extends [`gpui::Styled`] with specific styling methods.
pub trait StyledExt: Styled + Sized {
    /// Apply self into a horizontal flex layout.
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    /// Apply self into a vertical flex layout.
    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }

    /// Render a border with a width of 1px, color ring color
    fn outline(self, _window: &Window, cx: &App) -> Self {
        self.border_color(cx.theme().accent.step(cx, ColorScaleStep::NINE))
    }

    /// Wraps the element in a ScrollView.
    ///
    /// Current this is only have a vertical scrollbar.
    fn scrollable(self, view_id: EntityId, axis: ScrollbarAxis) -> Scrollable<Self>
    where
        Self: Element,
    {
        Scrollable::new(view_id, self, axis)
    }

    font_weight!(font_thin, THIN);
    font_weight!(font_extralight, EXTRA_LIGHT);
    font_weight!(font_light, LIGHT);
    font_weight!(font_normal, NORMAL);
    font_weight!(font_medium, MEDIUM);
    font_weight!(font_semibold, SEMIBOLD);
    font_weight!(font_bold, BOLD);
    font_weight!(font_extrabold, EXTRA_BOLD);
    font_weight!(font_black, BLACK);

    /// Set as Popover style
    fn popover_style(self, cx: &mut App) -> Self {
        self.bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().base.step(cx, ColorScaleStep::SIX))
            .shadow_lg()
            .rounded_lg()
    }
}

impl<E: Styled> StyledExt for E {}

/// A size for elements.
#[derive(Clone, Default, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub enum Size {
    Size(Pixels),
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
}

impl From<Pixels> for Size {
    fn from(size: Pixels) -> Self {
        Size::Size(size)
    }
}

/// A trait for defining element that can be selected.
pub trait Selectable: Sized {
    fn element_id(&self) -> &ElementId;
    /// Set the selected state of the element.
    fn selected(self, selected: bool) -> Self;
}

/// A trait for defining element that can be disabled.
pub trait Disableable {
    /// Set the disabled state of the element.
    fn disabled(self, disabled: bool) -> Self;
}

/// A trait for setting the size of an element.
pub trait Sizable: Sized {
    /// Set the ui::Size of this element.
    ///
    /// Also can receive a `ButtonSize` to convert to `IconSize`,
    /// Or a `Pixels` to set a custom size: `px(30.)`
    fn with_size(self, size: impl Into<Size>) -> Self;

    /// Set to Size::XSmall
    fn xsmall(self) -> Self {
        self.with_size(Size::XSmall)
    }

    /// Set to Size::Small
    fn small(self) -> Self {
        self.with_size(Size::Small)
    }

    /// Set to Size::Medium
    fn medium(self) -> Self {
        self.with_size(Size::Medium)
    }

    /// Set to Size::Large
    fn large(self) -> Self {
        self.with_size(Size::Large)
    }
}

#[allow(unused)]
pub trait StyleSized<T: Styled> {
    fn input_text_size(self, size: Size) -> Self;
    fn input_size(self, size: Size) -> Self;
    fn input_pl(self, size: Size) -> Self;
    fn input_pr(self, size: Size) -> Self;
    fn input_px(self, size: Size) -> Self;
    fn input_py(self, size: Size) -> Self;
    fn input_h(self, size: Size) -> Self;
    fn list_size(self, size: Size) -> Self;
    fn list_px(self, size: Size) -> Self;
    fn list_py(self, size: Size) -> Self;
    /// Apply size with the given `Size`.
    fn size_with(self, size: Size) -> Self;
}

impl<T: Styled> StyleSized<T> for T {
    fn input_text_size(self, size: Size) -> Self {
        match size {
            Size::XSmall => self.text_xs(),
            Size::Small => self.text_sm(),
            Size::Medium => self.text_base(),
            Size::Large => self.text_lg(),
            Size::Size(size) => self.text_size(size),
        }
    }

    fn input_size(self, size: Size) -> Self {
        self.input_px(size).input_py(size).input_h(size)
    }

    fn input_pl(self, size: Size) -> Self {
        match size {
            Size::Large => self.pl_5(),
            Size::Medium => self.pl_3(),
            _ => self.pl_2(),
        }
    }

    fn input_pr(self, size: Size) -> Self {
        match size {
            Size::Large => self.pr_5(),
            Size::Medium => self.pr_3(),
            _ => self.pr_2(),
        }
    }

    fn input_px(self, size: Size) -> Self {
        match size {
            Size::Large => self.px_5(),
            Size::Medium => self.px_3(),
            _ => self.px_2(),
        }
    }

    fn input_py(self, size: Size) -> Self {
        match size {
            Size::Large => self.py_5(),
            Size::Medium => self.py_2(),
            _ => self.py_1(),
        }
    }

    fn input_h(self, size: Size) -> Self {
        match size {
            Size::Large => self.h_11(),
            Size::Medium => self.h_8(),
            _ => self.h(px(26.)),
        }
        .input_text_size(size)
    }

    fn list_size(self, size: Size) -> Self {
        self.list_px(size).list_py(size).input_text_size(size)
    }

    fn list_px(self, size: Size) -> Self {
        match size {
            Size::Small => self.px_2(),
            _ => self.px_3(),
        }
    }

    fn list_py(self, size: Size) -> Self {
        match size {
            Size::Large => self.py_2(),
            Size::Medium => self.py_1(),
            Size::Small => self.py_0p5(),
            _ => self.py_1(),
        }
    }

    fn size_with(self, size: Size) -> Self {
        match size {
            Size::Large => self.size_11(),
            Size::Medium => self.size_8(),
            Size::Small => self.size_5(),
            Size::XSmall => self.size_4(),
            Size::Size(size) => self.size(size),
        }
    }
}

pub trait AxisExt {
    fn is_horizontal(&self) -> bool;
    fn is_vertical(&self) -> bool;
}

impl AxisExt for Axis {
    fn is_horizontal(&self) -> bool {
        self == &Axis::Horizontal
    }

    fn is_vertical(&self) -> bool {
        self == &Axis::Vertical
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    Top,
    Bottom,
    Left,
    Right,
}

impl Display for Placement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Placement::Top => write!(f, "Top"),
            Placement::Bottom => write!(f, "Bottom"),
            Placement::Left => write!(f, "Left"),
            Placement::Right => write!(f, "Right"),
        }
    }
}

impl Placement {
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Placement::Left | Placement::Right)
    }

    pub fn is_vertical(&self) -> bool {
        matches!(self, Placement::Top | Placement::Bottom)
    }

    pub fn axis(&self) -> Axis {
        match self {
            Placement::Top | Placement::Bottom => Axis::Vertical,
            Placement::Left | Placement::Right => Axis::Horizontal,
        }
    }
}

/// A enum for defining the side of the element.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub(crate) fn is_left(&self) -> bool {
        matches!(self, Self::Left)
    }
}

/// A trait for defining element that can be collapsed.
pub trait Collapsible {
    fn collapsed(self, collapsed: bool) -> Self;
    fn is_collapsed(&self) -> bool;
}
