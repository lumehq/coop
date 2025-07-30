use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, relative, AnyElement, App, ClickEvent, Div, ElementId, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use theme::ActiveTheme;

use crate::indicator::Indicator;
use crate::tooltip::Tooltip;
use crate::{Disableable, Icon, Selectable, Sizable, Size, StyledExt};

pub enum ButtonRounded {
    Normal,
    Full,
}

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
        self.with_variant(ButtonVariant::Ghost)
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
    Ghost,
    Transparent,
    Custom(ButtonCustomVariant),
}

impl Default for ButtonVariant {
    fn default() -> Self {
        Self::Primary
    }
}

type OnClick = Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>;

/// A Button element.
#[derive(IntoElement)]
pub struct Button {
    pub base: Div,
    id: ElementId,

    icon: Option<Icon>,
    label: Option<SharedString>,
    tooltip: Option<SharedString>,
    children: Vec<AnyElement>,

    variant: ButtonVariant,
    rounded: ButtonRounded,
    size: Size,

    disabled: bool,
    reverse: bool,
    bold: bool,
    cta: bool,

    loading: bool,
    loading_icon: Option<Icon>,

    on_click: OnClick,

    pub(crate) selected: bool,
    pub(crate) stop_propagation: bool,
}

impl From<Button> for AnyElement {
    fn from(button: Button) -> Self {
        button.into_any_element()
    }
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().flex_shrink_0(),
            id: id.into(),
            icon: None,
            label: None,
            disabled: false,
            selected: false,
            variant: ButtonVariant::default(),
            rounded: ButtonRounded::Normal,
            size: Size::Medium,
            tooltip: None,
            on_click: None,
            stop_propagation: true,
            loading: false,
            reverse: false,
            bold: false,
            cta: false,
            children: Vec::new(),
            loading_icon: None,
        }
    }

    /// Set the border radius of the Button.
    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
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

    /// Set the stop propagation of the button.
    pub fn stop_propagation(mut self, val: bool) -> Self {
        self.stop_propagation = val;
        self
    }

    /// Set the loading icon of the button.
    pub fn loading_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.loading_icon = Some(icon.into());
        self
    }

    /// Set the click handler of the button.
    pub fn on_click<C>(mut self, handler: C) -> Self
    where
        C: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Button {
    fn element_id(&self) -> &ElementId {
        &self.id
    }

    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
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
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
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
        let normal_style = style.normal(window, cx);
        let icon_size = match self.size {
            Size::Size(v) => Size::Size(v * 0.75),
            Size::Medium => Size::Small,
            _ => self.size,
        };

        self.base
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .overflow_hidden()
            .map(|this| match self.rounded {
                ButtonRounded::Normal => this.rounded(cx.theme().radius),
                ButtonRounded::Full => this.rounded_full(),
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
                                this.h_7().pl_1().pr_1p5()
                            } else {
                                this.h_7().px_1()
                            }
                        }
                        Size::Small => {
                            if self.icon.is_some() {
                                this.h_7().pl_2().pr_2p5()
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
            .when(self.selected, |this| {
                let selected_style = style.selected(window, cx);
                this.bg(selected_style.bg).text_color(selected_style.fg)
            })
            .when(!self.disabled && !self.selected, |this| {
                this.bg(normal_style.bg)
                    .hover(|this| {
                        let hover_style = style.hovered(window, cx);
                        this.bg(hover_style.bg).text_color(hover_style.fg)
                    })
                    .active(|this| {
                        let active_style = style.active(window, cx);
                        this.bg(active_style.bg).text_color(active_style.fg)
                    })
            })
            .when(self.disabled, |this| {
                let disabled_style = style.disabled(window, cx);
                this.cursor_not_allowed()
                    .bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .shadow_none()
            })
            .child({
                div()
                    .flex()
                    .when(self.reverse, |this| this.flex_row_reverse())
                    .id("label")
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .map(|this| match self.size {
                        Size::XSmall => this.gap_1(),
                        Size::Small => this.gap_1p5(),
                        _ => this.gap_2(),
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
            .when(self.loading, |this| this.bg(normal_style.bg.opacity(0.8)))
            .when_some(self.tooltip.clone(), |this, tooltip| {
                this.tooltip(move |window, cx| Tooltip::new(tooltip.clone(), window, cx).into())
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled && !self.loading),
                |this, on_click| {
                    let stop_propagation = self.stop_propagation;
                    this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.prevent_default();
                        if stop_propagation {
                            cx.stop_propagation();
                        }
                    })
                    .on_click(move |event, window, cx| {
                        (on_click)(event, window, cx);
                    })
                },
            )
    }
}

struct ButtonVariantStyle {
    bg: Hsla,
    fg: Hsla,
}

impl ButtonVariant {
    fn normal(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = self.bg_color(window, cx);
        let fg = self.text_color(window, cx);

        ButtonVariantStyle { bg, fg }
    }

    fn bg_color(&self, _window: &Window, cx: &App) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().element_background,
            ButtonVariant::Secondary => cx.theme().elevated_surface_background,
            ButtonVariant::Danger => cx.theme().danger_background,
            ButtonVariant::Warning => cx.theme().warning_background,
            ButtonVariant::Custom(colors) => colors.color,
            _ => cx.theme().ghost_element_background,
        }
    }

    fn text_color(&self, _window: &Window, cx: &App) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().element_foreground,
            ButtonVariant::Secondary => cx.theme().text_muted,
            ButtonVariant::Danger => cx.theme().danger_foreground,
            ButtonVariant::Warning => cx.theme().warning_foreground,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            ButtonVariant::Ghost => cx.theme().text_muted,
            ButtonVariant::Custom(colors) => colors.foreground,
        }
    }

    fn hovered(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_hover,
            ButtonVariant::Secondary => cx.theme().secondary_hover,
            ButtonVariant::Danger => cx.theme().danger_hover,
            ButtonVariant::Warning => cx.theme().warning_hover,
            ButtonVariant::Ghost => cx.theme().ghost_element_hover,
            ButtonVariant::Transparent => gpui::transparent_black(),
            ButtonVariant::Custom(colors) => colors.hover,
        };

        let fg = match self {
            ButtonVariant::Secondary => cx.theme().secondary_foreground,
            ButtonVariant::Ghost => cx.theme().text,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            _ => self.text_color(window, cx),
        };

        ButtonVariantStyle { bg, fg }
    }

    fn active(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_active,
            ButtonVariant::Secondary => cx.theme().secondary_active,
            ButtonVariant::Danger => cx.theme().danger_active,
            ButtonVariant::Warning => cx.theme().warning_active,
            ButtonVariant::Ghost => cx.theme().ghost_element_active,
            ButtonVariant::Transparent => gpui::transparent_black(),
            ButtonVariant::Custom(colors) => colors.active,
        };

        let fg = match self {
            ButtonVariant::Secondary => cx.theme().secondary_foreground,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            _ => self.text_color(window, cx),
        };

        ButtonVariantStyle { bg, fg }
    }

    fn selected(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_selected,
            ButtonVariant::Secondary => cx.theme().secondary_selected,
            ButtonVariant::Danger => cx.theme().danger_selected,
            ButtonVariant::Warning => cx.theme().warning_selected,
            ButtonVariant::Ghost => cx.theme().ghost_element_selected,
            ButtonVariant::Transparent => gpui::transparent_black(),
            ButtonVariant::Custom(colors) => colors.active,
        };

        let fg = match self {
            ButtonVariant::Secondary => cx.theme().secondary_foreground,
            ButtonVariant::Transparent => cx.theme().text_placeholder,
            _ => self.text_color(window, cx),
        };

        ButtonVariantStyle { bg, fg }
    }

    fn disabled(&self, _window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Ghost => cx.theme().ghost_element_disabled,
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
