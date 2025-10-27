use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, relative, AnyElement, App, ClickEvent, Div, ElementId, Hsla, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, SharedString, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
};
use theme::ActiveTheme;

use crate::indicator::Indicator;
use crate::tooltip::Tooltip;
use crate::{h_flex, Disableable, Icon, Selectable, Sizable, Size, StyledExt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ButtonCustomVariant {
    color: Hsla,
    foreground: Hsla,
    hover: Hsla,
    active: Hsla,
}

pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;

    /// With the primary style for the Button.
    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Primary)
    }

    /// With the secondary style for the Button.
    fn secondary(self) -> Self {
        self.with_variant(ButtonVariant::Secondary)
    }

    /// With the danger style for the Button.
    fn danger(self) -> Self {
        self.with_variant(ButtonVariant::Danger)
    }

    /// With the warning style for the Button.
    fn warning(self) -> Self {
        self.with_variant(ButtonVariant::Warning)
    }

    /// With the ghost style for the Button.
    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Ghost { alt: false })
    }

    /// With the ghost style for the Button.
    fn ghost_alt(self) -> Self {
        self.with_variant(ButtonVariant::Ghost { alt: true })
    }

    /// With the transparent style for the Button.
    fn transparent(self) -> Self {
        self.with_variant(ButtonVariant::Transparent)
    }

    /// With the custom style for the Button.
    fn custom(self, style: ButtonCustomVariant) -> Self {
        self.with_variant(ButtonVariant::Custom(style))
    }
}

impl ButtonCustomVariant {
    pub fn new(_window: &Window, cx: &App) -> Self {
        Self {
            color: cx.theme().element_background,
            foreground: cx.theme().element_foreground,
            hover: cx.theme().element_hover,
            active: cx.theme().element_active,
        }
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = color;
        self
    }

    pub fn foreground(mut self, color: Hsla) -> Self {
        self.foreground = color;
        self
    }

    pub fn hover(mut self, color: Hsla) -> Self {
        self.hover = color;
        self
    }

    pub fn active(mut self, color: Hsla) -> Self {
        self.active = color;
        self
    }
}

/// The variant of the Button.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Warning,
    Ghost { alt: bool },
    Transparent,
    Custom(ButtonCustomVariant),
}

impl Default for ButtonVariant {
    fn default() -> Self {
        Self::Primary
    }
}

/// A Button element.
#[derive(IntoElement)]
#[allow(clippy::type_complexity)]
pub struct Button {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,

    icon: Option<Icon>,
    label: Option<SharedString>,
    tooltip: Option<SharedString>,
    children: Vec<AnyElement>,

    variant: ButtonVariant,
    rounded: bool,
    size: Size,

    disabled: bool,
    reverse: bool,
    bold: bool,
    cta: bool,

    loading: bool,
    loading_icon: Option<Icon>,

    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    on_hover: Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>,

    tab_index: isize,
    tab_stop: bool,

    pub(crate) selected: bool,
}

impl From<Button> for AnyElement {
    fn from(button: Button) -> Self {
        button.into_any_element()
    }
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();

        Self {
            id: id.clone(),
            base: div().flex_shrink_0().id(id),
            style: StyleRefinement::default(),
            icon: None,
            label: None,
            disabled: false,
            selected: false,
            variant: ButtonVariant::default(),
            rounded: false,
            size: Size::Medium,
            tooltip: None,
            on_click: None,
            on_hover: None,
            loading: false,
            reverse: false,
            bold: false,
            cta: false,
            children: Vec::new(),
            loading_icon: None,
            tab_index: 0,
            tab_stop: true,
        }
    }

    /// Make the button rounded.
    pub fn rounded(mut self) -> Self {
        self.rounded = true;
        self
    }

    /// Set label to the Button, if no label is set, the button will be in Icon Button mode.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the icon of the button, if the Button have no label, the button well in Icon Button mode.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set the tooltip of the button.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Set true to show the loading indicator.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Set reverse the position between icon and label.
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    /// Set bold the button (label will be use the semi-bold font).
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Set the cta style of the button.
    pub fn cta(mut self) -> Self {
        self.cta = true;
        self
    }

    /// Set the loading icon of the button.
    pub fn loading_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.loading_icon = Some(icon.into());
        self
    }

    /// Add click handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Add hover handler, the bool parameter indicates whether the mouse is hovering.
    pub fn on_hover(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }

    /// Set the tab index of the button, it will be used to focus the button by tab key.
    ///
    /// Default is 0.
    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

    /// Set the tab stop of the button, if true, the button will be focusable by tab key.
    ///
    /// Default is true.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    #[inline]
    fn clickable(&self) -> bool {
        !(self.disabled || self.loading) && self.on_click.is_some()
    }

    #[inline]
    fn hoverable(&self) -> bool {
        !(self.disabled || self.loading) && self.on_hover.is_some()
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Button {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ButtonVariants for Button {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        self.children.extend(elements)
    }
}

impl InteractiveElement for Button {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let style: ButtonVariant = self.variant;
        let clickable = self.clickable();
        let hoverable = self.hoverable();
        let normal_style = style.normal(cx);
        let icon_size = match self.size {
            Size::Size(v) => Size::Size(v * 0.75),
            Size::Large => Size::Medium,
            _ => self.size,
        };

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();

        self.base
            .when(!self.disabled, |this| {
                this.track_focus(
                    &focus_handle
                        .tab_index(self.tab_index)
                        .tab_stop(self.tab_stop),
                )
            })
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .overflow_hidden()
            .map(|this| match self.rounded {
                false => this.rounded(cx.theme().radius),
                true => this.rounded_full(),
            })
            .map(|this| {
                if self.label.is_none() && self.children.is_empty() {
                    // Icon Button
                    match self.size {
                        Size::Size(px) => this.size(px),
                        Size::XSmall => {
                            if self.cta {
                                this.w_10().h_5()
                            } else {
                                this.size_5()
                            }
                        }
                        Size::Small => {
                            if self.cta {
                                this.w_12().h_6()
                            } else {
                                this.size_6()
                            }
                        }
                        Size::Medium => {
                            if self.cta {
                                this.w_12().h_7()
                            } else {
                                this.size_7()
                            }
                        }
                        _ => {
                            if self.cta {
                                this.w_16().h_9()
                            } else {
                                this.size_9()
                            }
                        }
                    }
                } else {
                    // Normal Button
                    match self.size {
                        Size::Size(size) => this.px(size * 0.2),
                        Size::XSmall => {
                            if self.icon.is_some() {
                                this.h_6().pl_2().pr_2p5()
                            } else if self.cta {
                                this.h_6().px_4()
                            } else {
                                this.h_6().px_2()
                            }
                        }
                        Size::Small => {
                            if self.icon.is_some() {
                                this.h_7().pl_2().pr_2p5()
                            } else if self.cta {
                                this.h_7().px_4()
                            } else {
                                this.h_7().px_2()
                            }
                        }
                        Size::Medium => {
                            if self.icon.is_some() {
                                this.h_8().pl_3().pr_3p5()
                            } else {
                                this.h_8().px_3()
                            }
                        }
                        Size::Large => {
                            if self.icon.is_some() {
                                this.h_10().px_3().pr_3p5()
                            } else {
                                this.h_10().px_3()
                            }
                        }
                    }
                }
            })
            .text_color(normal_style.fg)
            .when(!self.disabled && !self.selected, |this| {
                this.bg(normal_style.bg)
                    .hover(|this| {
                        let hover_style = style.hovered(cx);
                        this.bg(hover_style.bg).text_color(hover_style.fg)
                    })
                    .active(|this| {
                        let active_style = style.active(cx);
                        this.bg(active_style.bg).text_color(active_style.fg)
                    })
            })
            .when(self.selected, |this| {
                let selected_style = style.selected(cx);
                this.bg(selected_style.bg).text_color(selected_style.fg)
            })
            .when(self.disabled, |this| {
                let disabled_style = style.disabled(cx);
                this.cursor_not_allowed()
                    .bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
            })
            .refine_style(&self.style)
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                // Avoid focus on mouse down.
                window.prevent_default();
            })
            .when_some(self.on_click.filter(|_| clickable), |this, on_click| {
                this.on_click(move |event, window, cx| {
                    (on_click)(event, window, cx);
                })
            })
            .when_some(self.on_hover.filter(|_| hoverable), |this, on_hover| {
                this.on_hover(move |hovered, window, cx| {
                    (on_hover)(hovered, window, cx);
                })
            })
            .child({
                h_flex()
                    .id("label")
                    .when(self.reverse, |this| this.flex_row_reverse())
                    .justify_center()
                    .map(|this| match self.size {
                        Size::XSmall => this.text_xs().gap_1(),
                        Size::Small => this.text_sm().gap_1p5(),
                        _ => this.text_sm().gap_2(),
                    })
                    .when(!self.loading, |this| {
                        this.when_some(self.icon, |this, icon| {
                            this.child(icon.with_size(icon_size))
                        })
                    })
                    .when(self.loading, |this| {
                        this.child(
                            Indicator::new()
                                .when_some(self.loading_icon, |this, icon| this.icon(icon)),
                        )
                    })
                    .when_some(self.label, |this, label| {
                        this.child(
                            div()
                                .flex_none()
                                .line_height(relative(1.))
                                .child(label)
                                .when(self.bold, |this| this.font_semibold()),
                        )
                    })
                    .children(self.children)
            })
            .when(self.loading && !self.disabled, |this| {
                this.bg(normal_style.bg.opacity(0.8))
                    .text_color(normal_style.fg.opacity(0.8))
            })
            .when_some(self.tooltip.clone(), |this, tooltip| {
                this.tooltip(move |window, cx| Tooltip::new(tooltip.clone(), window, cx).into())
            })
    }
}

struct ButtonVariantStyle {
    bg: Hsla,
    fg: Hsla,
}

impl ButtonVariant {
    fn normal(&self, cx: &App) -> ButtonVariantStyle {
        let bg = self.bg_color(cx);
        let fg = self.text_color(cx);

        ButtonVariantStyle { bg, fg }
    }

    fn bg_color(&self, cx: &App) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().element_background,
            ButtonVariant::Secondary => cx.theme().elevated_surface_background,
            ButtonVariant::Danger => cx.theme().danger_background,
            ButtonVariant::Warning => cx.theme().warning_background,
            ButtonVariant::Ghost { alt } => {
                if *alt {
                    cx.theme().ghost_element_background_alt
                } else {
                    cx.theme().ghost_element_background
                }
            }
            ButtonVariant::Custom(colors) => colors.color,
            _ => gpui::transparent_black(),
        }
    }

    fn text_color(&self, cx: &App) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().element_foreground,
            ButtonVariant::Secondary => cx.theme().text_muted,
            ButtonVariant::Danger => cx.theme().danger_foreground,
            ButtonVariant::Warning => cx.theme().warning_foreground,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            ButtonVariant::Ghost { alt } => {
                if *alt {
                    cx.theme().text
                } else {
                    cx.theme().text_muted
                }
            }
            ButtonVariant::Custom(colors) => colors.foreground,
        }
    }

    fn hovered(&self, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_hover,
            ButtonVariant::Secondary => cx.theme().secondary_hover,
            ButtonVariant::Danger => cx.theme().danger_hover,
            ButtonVariant::Warning => cx.theme().warning_hover,
            ButtonVariant::Ghost { .. } => cx.theme().ghost_element_hover,
            ButtonVariant::Transparent => gpui::transparent_black(),
            ButtonVariant::Custom(colors) => colors.hover,
        };

        let fg = match self {
            ButtonVariant::Secondary => cx.theme().secondary_foreground,
            ButtonVariant::Ghost { .. } => cx.theme().text,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            _ => self.text_color(cx),
        };

        ButtonVariantStyle { bg, fg }
    }

    fn active(&self, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_active,
            ButtonVariant::Secondary => cx.theme().secondary_active,
            ButtonVariant::Danger => cx.theme().danger_active,
            ButtonVariant::Warning => cx.theme().warning_active,
            ButtonVariant::Ghost { .. } => cx.theme().ghost_element_active,
            ButtonVariant::Transparent => gpui::transparent_black(),
            ButtonVariant::Custom(colors) => colors.active,
        };

        let fg = match self {
            ButtonVariant::Secondary => cx.theme().secondary_foreground,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            _ => self.text_color(cx),
        };

        ButtonVariantStyle { bg, fg }
    }

    fn selected(&self, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_selected,
            ButtonVariant::Secondary => cx.theme().secondary_selected,
            ButtonVariant::Danger => cx.theme().danger_selected,
            ButtonVariant::Warning => cx.theme().warning_selected,
            ButtonVariant::Ghost { .. } => cx.theme().ghost_element_selected,
            ButtonVariant::Transparent => gpui::transparent_black(),
            ButtonVariant::Custom(colors) => colors.active,
        };

        let fg = match self {
            ButtonVariant::Secondary => cx.theme().secondary_foreground,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            _ => self.text_color(cx),
        };

        ButtonVariantStyle { bg, fg }
    }

    fn disabled(&self, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Danger => cx.theme().danger_disabled,
            ButtonVariant::Warning => cx.theme().warning_disabled,
            ButtonVariant::Ghost { .. } => cx.theme().ghost_element_disabled,
            ButtonVariant::Secondary => cx.theme().secondary_disabled,
            _ => cx.theme().element_disabled,
        };

        let fg = match self {
            ButtonVariant::Primary => cx.theme().text_muted, // TODO: use a different color?
            _ => cx.theme().text_muted,
        };

        ButtonVariantStyle { bg, fg }
    }
}
