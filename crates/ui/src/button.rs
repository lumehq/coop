use gpui::{
    div, prelude::FluentBuilder as _, relative, AnyElement, App, ClickEvent, Div, ElementId, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use theme::ActiveTheme;

use crate::{
    indicator::Indicator, tooltip::Tooltip, Disableable, Icon, Selectable, Sizable, Size, StyledExt,
};

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

    /// With the ghost style for the Button.
    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Ghost)
    }

    /// With the link style for the Button.
    fn link(self) -> Self {
        self.with_variant(ButtonVariant::Link)
    }

    /// With the text style for the Button, it will no padding look like a normal text.
    fn text(self) -> Self {
        self.with_variant(ButtonVariant::Text)
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
    Ghost,
    Link,
    Text,
    Custom(ButtonCustomVariant),
}

impl Default for ButtonVariant {
    fn default() -> Self {
        Self::Primary
    }
}

impl ButtonVariant {
    fn is_link(&self) -> bool {
        matches!(self, Self::Link)
    }

    fn is_text(&self) -> bool {
        matches!(self, Self::Text)
    }

    fn no_padding(&self) -> bool {
        self.is_link() || self.is_text()
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
    children: Vec<AnyElement>,
    disabled: bool,
    variant: ButtonVariant,
    rounded: ButtonRounded,
    size: Size,
    reverse: bool,
    bold: bool,
    tooltip: Option<SharedString>,
    on_click: OnClick,
    loading: bool,
    loading_icon: Option<Icon>,
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

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn stop_propagation(mut self, val: bool) -> Self {
        self.stop_propagation = val;
        self
    }

    pub fn loading_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.loading_icon = Some(icon.into());
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
            .when(!style.no_padding(), |this| {
                if self.label.is_none() && self.children.is_empty() {
                    // Icon Button
                    match self.size {
                        Size::Size(px) => this.size(px),
                        Size::XSmall => this.size_5(),
                        Size::Small => this.size_6(),
                        _ => this.size_9(),
                    }
                } else {
                    // Normal Button
                    match self.size {
                        Size::Size(size) => this.px(size * 0.2),
                        Size::XSmall => this.h_6().px_1p5(),
                        Size::Small => this.h_7().px_2(),
                        Size::Large => this.h_10().px_3(),
                        _ => this.h_9().px_2(),
                    }
                }
            })
            .when(self.selected, |this| {
                let selected_style = style.selected(window, cx);
                this.bg(selected_style.bg).text_color(selected_style.fg)
            })
            .when(!self.disabled && !self.selected, |this| {
                this.bg(normal_style.bg)
                    .when(normal_style.underline, |this| this.text_decoration_1())
                    .hover(|this| {
                        let hover_style = style.hovered(window, cx);
                        this.bg(hover_style.bg)
                    })
                    .active(|this| {
                        let active_style = style.active(window, cx);
                        this.bg(active_style.bg).text_color(active_style.fg)
                    })
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
            .when(self.disabled, |this| {
                let disabled_style = style.disabled(window, cx);
                this.cursor_not_allowed()
                    .bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .shadow_none()
            })
            .text_color(normal_style.fg)
            .child({
                div()
                    .flex()
                    .when(self.reverse, |this| this.flex_row_reverse())
                    .id("label")
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .map(|this| match self.size {
                        Size::XSmall => this.gap_0p5(),
                        Size::Small => this.gap_1(),
                        _ => this.gap_2().font_medium(),
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
    }
}

struct ButtonVariantStyle {
    bg: Hsla,
    fg: Hsla,
    underline: bool,
}

impl ButtonVariant {
    fn bg_color(&self, _window: &Window, cx: &App) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().element_background,
            ButtonVariant::Custom(colors) => colors.color,
            _ => cx.theme().ghost_element_background,
        }
    }

    fn text_color(&self, _window: &Window, cx: &App) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().element_foreground,
            ButtonVariant::Link => cx.theme().text_accent,
            ButtonVariant::Ghost => cx.theme().text_muted,
            ButtonVariant::Custom(colors) => colors.foreground,
            _ => cx.theme().text,
        }
    }

    fn underline(&self, _window: &Window, _cx: &App) -> bool {
        matches!(self, ButtonVariant::Link)
    }

    fn normal(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = self.bg_color(window, cx);
        let fg = self.text_color(window, cx);
        let underline = self.underline(window, cx);

        ButtonVariantStyle { bg, fg, underline }
    }

    fn hovered(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_hover,
            ButtonVariant::Ghost => cx.theme().ghost_element_hover,
            ButtonVariant::Link => cx.theme().ghost_element_background,
            ButtonVariant::Text => cx.theme().ghost_element_background,
            ButtonVariant::Custom(colors) => colors.hover,
        };
        let fg = match self {
            ButtonVariant::Ghost => cx.theme().text,
            ButtonVariant::Link => cx.theme().text_accent,
            _ => self.text_color(window, cx),
        };
        let underline = self.underline(window, cx);

        ButtonVariantStyle { bg, fg, underline }
    }

    fn active(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_active,
            ButtonVariant::Ghost => cx.theme().ghost_element_active,
            ButtonVariant::Custom(colors) => colors.active,
            _ => cx.theme().ghost_element_background,
        };
        let fg = match self {
            ButtonVariant::Link => cx.theme().text_accent,
            ButtonVariant::Text => cx.theme().text,
            _ => self.text_color(window, cx),
        };
        let underline = self.underline(window, cx);

        ButtonVariantStyle { bg, fg, underline }
    }

    fn selected(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().element_selected,
            ButtonVariant::Ghost => cx.theme().ghost_element_selected,
            ButtonVariant::Custom(colors) => colors.active,
            _ => cx.theme().ghost_element_background,
        };
        let fg = match self {
            ButtonVariant::Link => cx.theme().text_accent,
            ButtonVariant::Text => cx.theme().text,
            _ => self.text_color(window, cx),
        };
        let underline = self.underline(window, cx);

        ButtonVariantStyle { bg, fg, underline }
    }

    fn disabled(&self, window: &Window, cx: &App) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Link | ButtonVariant::Ghost | ButtonVariant::Text => {
                cx.theme().ghost_element_disabled
            }
            _ => cx.theme().element_disabled,
        };
        let fg = match self {
            ButtonVariant::Primary => cx.theme().text_muted, // TODO: use a different color?
            _ => cx.theme().text_muted,
        };
        let underline = self.underline(window, cx);

        ButtonVariantStyle { bg, fg, underline }
    }
}
