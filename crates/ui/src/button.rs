use crate::{
    indicator::Indicator,
    theme::{scale::ColorScaleStep, ActiveTheme},
    tooltip::Tooltip,
    Disableable, Icon, Selectable, Sizable, Size, StyledExt,
};
use gpui::{
    div, prelude::FluentBuilder as _, px, relative, AnyElement, ClickEvent, Corners, Div, Edges,
    ElementId, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, WindowContext,
};

pub enum ButtonRounded {
    None,
    Small,
    Medium,
    Large,
    Size(Pixels),
}

impl From<Pixels> for ButtonRounded {
    fn from(px: Pixels) -> Self {
        ButtonRounded::Size(px)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ButtonCustomVariant {
    color: Hsla,
    foreground: Hsla,
    border: Hsla,
    shadow: bool,
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
    pub fn new(cx: &WindowContext) -> Self {
        Self {
            color: cx.theme().accent.step(cx, ColorScaleStep::NINE),
            foreground: cx.theme().accent.step(cx, ColorScaleStep::ONE),
            border: cx.theme().accent.step(cx, ColorScaleStep::TEN),
            hover: cx.theme().accent.step(cx, ColorScaleStep::TEN),
            active: cx.theme().accent.step(cx, ColorScaleStep::ELEVEN),
            shadow: true,
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

    pub fn border(mut self, color: Hsla) -> Self {
        self.border = color;
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

    pub fn shadow(mut self, shadow: bool) -> Self {
        self.shadow = shadow;
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

type OnClick = Option<Box<dyn Fn(&ClickEvent, &mut WindowContext) + 'static>>;

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
    border_corners: Corners<bool>,
    border_edges: Edges<bool>,
    size: Size,
    reverse: bool,
    bold: bool,
    centered: bool,
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
            rounded: ButtonRounded::Medium,
            border_corners: Corners::all(true),
            border_edges: Edges::all(true),
            size: Size::Medium,
            tooltip: None,
            on_click: None,
            stop_propagation: true,
            loading: false,
            reverse: false,
            bold: false,
            centered: true,
            children: Vec::new(),
            loading_icon: None,
        }
    }

    /// Set the border radius of the Button.
    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
        self
    }

    /// Set the border corners side of the Button.
    pub(crate) fn border_corners(mut self, corners: impl Into<Corners<bool>>) -> Self {
        self.border_corners = corners.into();
        self
    }

    /// Set the border edges of the Button.
    pub(crate) fn border_edges(mut self, edges: impl Into<Edges<bool>>) -> Self {
        self.border_edges = edges.into();
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

    pub fn not_centered(mut self) -> Self {
        self.centered = false;
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut WindowContext) + 'static) -> Self {
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
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let style: ButtonVariant = self.variant;
        let normal_style = style.normal(cx);
        let icon_size = match self.size {
            Size::Size(v) => Size::Size(v * 0.75),
            _ => self.size,
        };

        self.base
            .id(self.id)
            .flex()
            .items_center()
            .when(self.centered, |this| this.justify_center())
            .cursor_pointer()
            .overflow_hidden()
            .when(cx.theme().shadow && normal_style.shadow, |this| {
                this.shadow_sm()
            })
            .when(!style.no_padding(), |this| {
                if self.label.is_none() && self.children.is_empty() {
                    // Icon Button
                    match self.size {
                        Size::Size(px) => this.size(px),
                        Size::XSmall => this.size_5(),
                        Size::Small => this.size_6(),
                        Size::Medium => this.size_8(),
                        Size::Large => this.size_9(),
                    }
                } else {
                    // Normal Button
                    match self.size {
                        Size::Size(size) => this.px(size * 0.2),
                        Size::XSmall => this.h_6().px_0p5(),
                        Size::Small => this.h_7().px_2(),
                        _ => this.h_8().px_3(),
                    }
                }
            })
            .when(
                self.border_corners.top_left && self.border_corners.bottom_left,
                |this| match self.rounded {
                    ButtonRounded::Small => this.rounded_l(px(cx.theme().radius * 0.5)),
                    ButtonRounded::Medium => this.rounded_l(px(cx.theme().radius)),
                    ButtonRounded::Large => this.rounded_l(px(cx.theme().radius * 1.6)),
                    ButtonRounded::Size(px) => this.rounded_l(px),
                    ButtonRounded::None => this.rounded_none(),
                },
            )
            .when(
                self.border_corners.top_right && self.border_corners.bottom_right,
                |this| match self.rounded {
                    ButtonRounded::Small => this.rounded_r(px(cx.theme().radius * 0.5)),
                    ButtonRounded::Medium => this.rounded_r(px(cx.theme().radius)),
                    ButtonRounded::Large => this.rounded_r(px(cx.theme().radius * 1.6)),
                    ButtonRounded::Size(px) => this.rounded_r(px),
                    ButtonRounded::None => this.rounded_none(),
                },
            )
            .when(self.border_edges.left, |this| this.border_l_1())
            .when(self.border_edges.right, |this| this.border_r_1())
            .when(self.border_edges.top, |this| this.border_t_1())
            .when(self.border_edges.bottom, |this| this.border_b_1())
            .text_color(normal_style.fg)
            .when(self.selected, |this| {
                let selected_style = style.selected(cx);
                this.bg(selected_style.bg)
                    .border_color(selected_style.border)
                    .text_color(selected_style.fg)
            })
            .when(!self.disabled && !self.selected, |this| {
                this.border_color(normal_style.border)
                    .bg(normal_style.bg)
                    .when(normal_style.underline, |this| this.text_decoration_1())
                    .hover(|this| {
                        let hover_style = style.hovered(cx);
                        this.bg(hover_style.bg).border_color(hover_style.border)
                    })
                    .active(|this| {
                        let active_style = style.active(cx);
                        this.bg(active_style.bg)
                            .border_color(active_style.border)
                            .text_color(active_style.fg)
                    })
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled && !self.loading),
                |this, on_click| {
                    let stop_propagation = self.stop_propagation;
                    this.on_mouse_down(MouseButton::Left, move |_, cx| {
                        cx.prevent_default();
                        if stop_propagation {
                            cx.stop_propagation();
                        }
                    })
                    .on_click(move |event, cx| {
                        (on_click)(event, cx);
                    })
                },
            )
            .when(self.disabled, |this| {
                let disabled_style = style.disabled(cx);
                this.cursor_not_allowed()
                    .bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .border_color(disabled_style.border)
                    .shadow_none()
            })
            .child({
                div()
                    .flex()
                    .when(self.reverse, |this| this.flex_row_reverse())
                    .id("label")
                    .items_center()
                    .justify_center()
                    .map(|this| match self.size {
                        Size::XSmall => this.gap_0p5().text_xs(),
                        Size::Small => this.gap_1().text_xs(),
                        _ => this.gap_2().text_xs(),
                    })
                    .when(!self.loading, |this| {
                        this.when_some(self.icon, |this, icon| {
                            this.child(icon.with_size(icon_size))
                        })
                    })
                    .when(self.loading, |this| {
                        this.child(
                            Indicator::new()
                                .with_size(self.size)
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
                this.tooltip(move |cx| Tooltip::new(tooltip.clone(), cx).into())
            })
    }
}

struct ButtonVariantStyle {
    bg: Hsla,
    border: Hsla,
    fg: Hsla,
    underline: bool,
    shadow: bool,
}

impl ButtonVariant {
    fn bg_color(&self, cx: &WindowContext) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().accent.step(cx, ColorScaleStep::NINE),
            ButtonVariant::Custom(colors) => colors.color,
            _ => cx.theme().transparent,
        }
    }

    fn text_color(&self, cx: &WindowContext) -> Hsla {
        match self {
            ButtonVariant::Primary => match cx.theme().accent.name().to_string().as_str() {
                "Sky" => cx.theme().base.darken(cx),
                "Mint" => cx.theme().base.darken(cx),
                "Lime" => cx.theme().base.darken(cx),
                "Amber" => cx.theme().base.darken(cx),
                "Yellow" => cx.theme().base.darken(cx),
                _ => cx.theme().accent.step(cx, ColorScaleStep::ONE),
            },
            ButtonVariant::Link => cx.theme().accent.step(cx, ColorScaleStep::NINE),
            ButtonVariant::Ghost => cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
            ButtonVariant::Custom(colors) => colors.foreground,
            _ => cx.theme().base.step(cx, ColorScaleStep::TWELVE),
        }
    }

    fn border_color(&self, cx: &WindowContext) -> Hsla {
        match self {
            ButtonVariant::Primary => cx.theme().accent.step(cx, ColorScaleStep::NINE),
            ButtonVariant::Ghost | ButtonVariant::Link | ButtonVariant::Text => {
                cx.theme().transparent
            }
            ButtonVariant::Custom(colors) => colors.border,
        }
    }

    fn underline(&self, _: &WindowContext) -> bool {
        matches!(self, ButtonVariant::Link)
    }

    fn shadow(&self, _: &WindowContext) -> bool {
        match self {
            ButtonVariant::Primary => true,
            ButtonVariant::Custom(c) => c.shadow,
            _ => false,
        }
    }

    fn normal(&self, cx: &WindowContext) -> ButtonVariantStyle {
        let bg = self.bg_color(cx);
        let border = self.border_color(cx);
        let fg = self.text_color(cx);
        let underline = self.underline(cx);
        let shadow = self.shadow(cx);

        ButtonVariantStyle {
            bg,
            border,
            fg,
            underline,
            shadow,
        }
    }

    fn hovered(&self, cx: &WindowContext) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().accent.step(cx, ColorScaleStep::TEN),
            ButtonVariant::Ghost => cx.theme().base.step(cx, ColorScaleStep::FOUR),
            ButtonVariant::Link => cx.theme().transparent,
            ButtonVariant::Text => cx.theme().transparent,
            ButtonVariant::Custom(colors) => colors.hover,
        };
        let border = self.border_color(cx);
        let fg = match self {
            ButtonVariant::Ghost => cx.theme().base.step(cx, ColorScaleStep::TWELVE),
            ButtonVariant::Link => cx.theme().accent.step(cx, ColorScaleStep::TEN),
            _ => self.text_color(cx),
        };
        let underline = self.underline(cx);
        let shadow = self.shadow(cx);

        ButtonVariantStyle {
            bg,
            border,
            fg,
            underline,
            shadow,
        }
    }

    fn active(&self, cx: &WindowContext) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().accent.step(cx, ColorScaleStep::TEN),
            ButtonVariant::Ghost => cx.theme().base.step(cx, ColorScaleStep::THREE),
            ButtonVariant::Link => cx.theme().transparent,
            ButtonVariant::Text => cx.theme().transparent,
            ButtonVariant::Custom(colors) => colors.active,
        };

        let fg = match self {
            ButtonVariant::Link => cx.theme().accent.step(cx, ColorScaleStep::NINE),
            ButtonVariant::Text => cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
            _ => self.text_color(cx),
        };

        let border = self.border_color(cx);
        let underline = self.underline(cx);
        let shadow = self.shadow(cx);

        ButtonVariantStyle {
            bg,
            border,
            fg,
            underline,
            shadow,
        }
    }

    fn selected(&self, cx: &WindowContext) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Primary => cx.theme().accent.step(cx, ColorScaleStep::TEN),
            ButtonVariant::Ghost => cx.theme().base.step(cx, ColorScaleStep::THREE),
            ButtonVariant::Link => cx.theme().transparent,
            ButtonVariant::Text => cx.theme().transparent,
            ButtonVariant::Custom(colors) => colors.active,
        };

        let fg = match self {
            ButtonVariant::Link => cx.theme().accent.step(cx, ColorScaleStep::TEN),
            ButtonVariant::Text => cx.theme().accent.step(cx, ColorScaleStep::TEN),
            _ => self.text_color(cx),
        };

        let border = self.border_color(cx);
        let underline = self.underline(cx);
        let shadow = self.shadow(cx);

        ButtonVariantStyle {
            bg,
            border,
            fg,
            underline,
            shadow,
        }
    }

    fn disabled(&self, cx: &WindowContext) -> ButtonVariantStyle {
        let bg = match self {
            ButtonVariant::Link | ButtonVariant::Ghost | ButtonVariant::Text => {
                cx.theme().transparent
            }
            _ => cx.theme().base.step(cx, ColorScaleStep::THREE),
        };

        let fg = cx.theme().base.step(cx, ColorScaleStep::ELEVEN);
        let border = bg;
        let underline = self.underline(cx);
        let shadow = false;

        ButtonVariantStyle {
            bg,
            border,
            fg,
            underline,
            shadow,
        }
    }
}
