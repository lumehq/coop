use std::rc::Rc;

use gpui::{
    black, div, prelude::FluentBuilder as _, px, relative, white, AnyElement, App, ClickEvent, Div,
    Element, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels,
    RenderOnce, Rgba, Stateful, StatefulInteractiveElement as _, Style, Styled, Window,
};
use theme::ActiveTheme;

use crate::{h_flex, Icon, IconName, InteractiveElementExt as _, Sizable as _};

const HEIGHT: Pixels = px(34.);
const TITLE_BAR_HEIGHT: Pixels = px(34.);
#[cfg(target_os = "macos")]
const TITLE_BAR_LEFT_PADDING: Pixels = px(80.);
#[cfg(not(target_os = "macos"))]
const TITLE_BAR_LEFT_PADDING: Pixels = px(12.);

type OnCloseWindow = Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>;

/// TitleBar used to customize the appearance of the title bar.
///
/// We can put some elements inside the title bar.
#[derive(IntoElement)]
pub struct TitleBar {
    base: Stateful<Div>,
    children: Vec<AnyElement>,
    on_close_window: OnCloseWindow,
}

impl TitleBar {
    pub fn new() -> Self {
        Self {
            base: div().id("title-bar").pl(TITLE_BAR_LEFT_PADDING),
            children: Vec::new(),
            on_close_window: None,
        }
    }

    /// Add custom for close window event, default is None, then click X button will call `window.remove_window()`.
    /// Linux only, this will do nothing on other platforms.
    pub fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if cfg!(target_os = "linux") {
            self.on_close_window = Some(Rc::new(Box::new(f)));
        }
        self
    }
}

impl Default for TitleBar {
    fn default() -> Self {
        Self::new()
    }
}

// The Windows control buttons have a fixed width of 35px.
//
// We don't need implementation the click event for the control buttons.
// If user clicked in the bounds, the window event will be triggered.
#[derive(IntoElement, Clone)]
enum Control {
    Minimize,
    Restore,
    Maximize,
    Close { on_close_window: OnCloseWindow },
}

impl Control {
    fn minimize() -> Self {
        Self::Minimize
    }

    fn restore() -> Self {
        Self::Restore
    }

    fn maximize() -> Self {
        Self::Maximize
    }

    fn close(on_close_window: OnCloseWindow) -> Self {
        Self::Close { on_close_window }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Restore => "restore",
            Self::Maximize => "maximize",
            Self::Close { .. } => "close",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Restore => IconName::WindowRestore,
            Self::Maximize => IconName::WindowMaximize,
            Self::Close { .. } => IconName::WindowClose,
        }
    }

    fn is_close(&self) -> bool {
        matches!(self, Self::Close { .. })
    }

    fn fg(&self, _window: &Window, cx: &App) -> Hsla {
        if cx.theme().mode.is_dark() {
            white()
        } else {
            black()
        }
    }

    fn hover_fg(&self, _window: &Window, cx: &App) -> Hsla {
        if self.is_close() || cx.theme().mode.is_dark() {
            white()
        } else {
            black()
        }
    }

    fn hover_bg(&self, _window: &Window, cx: &App) -> Rgba {
        if self.is_close() {
            Rgba {
                r: 232.0 / 255.0,
                g: 17.0 / 255.0,
                b: 32.0 / 255.0,
                a: 1.0,
            }
        } else if cx.theme().mode.is_dark() {
            Rgba {
                r: 0.9,
                g: 0.9,
                b: 0.9,
                a: 0.1,
            }
        } else {
            Rgba {
                r: 0.1,
                g: 0.1,
                b: 0.1,
                a: 0.2,
            }
        }
    }
}

impl RenderOnce for Control {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fg = self.fg(window, cx);
        let hover_fg = self.hover_fg(window, cx);
        let hover_bg = self.hover_bg(window, cx);
        let icon = self.clone();
        let is_linux = cfg!(target_os = "linux");
        let on_close_window = match &icon {
            Control::Close { on_close_window } => on_close_window.clone(),
            _ => None,
        };

        div()
            .id(self.id())
            .flex()
            .cursor_pointer()
            .w(TITLE_BAR_HEIGHT)
            .h_full()
            .justify_center()
            .content_center()
            .items_center()
            .text_color(fg)
            .when(is_linux, |this| {
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| match icon {
                    Self::Minimize => window.minimize_window(),
                    Self::Restore => window.zoom_window(),
                    Self::Maximize => window.zoom_window(),
                    Self::Close { .. } => {
                        if let Some(f) = on_close_window.clone() {
                            f(&ClickEvent::default(), window, cx);
                        } else {
                            window.remove_window();
                        }
                    }
                })
            })
            .hover(|style| style.bg(hover_bg).text_color(hover_fg))
            .active(|style| style.bg(hover_bg))
            .child(Icon::new(self.icon()).small())
    }
}

#[derive(IntoElement)]
struct WindowControls {
    on_close_window: OnCloseWindow,
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        if cfg!(target_os = "macos") {
            return div().id("window-controls");
        }

        h_flex()
            .id("window-controls")
            .items_center()
            .flex_shrink_0()
            .h_full()
            .child(
                h_flex()
                    .justify_center()
                    .content_stretch()
                    .h_full()
                    .child(Control::minimize())
                    .child(if window.is_maximized() {
                        Control::restore()
                    } else {
                        Control::maximize()
                    }),
            )
            .child(Control::close(self.on_close_window))
    }
}

impl Styled for TitleBar {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_linux = cfg!(target_os = "linux");

        div().flex_shrink_0().child(
            self.base
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(HEIGHT)
                .bg(cx.theme().title_bar)
                .when(window.is_fullscreen(), |this| this.pl(px(12.)))
                .on_double_click(|_, window, _cx| window.zoom_window())
                .child(
                    h_flex()
                        .h_full()
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .when(is_linux, |this| {
                            this.child(
                                div()
                                    .top_0()
                                    .left_0()
                                    .absolute()
                                    .size_full()
                                    .h_full()
                                    .child(TitleBarElement {}),
                            )
                        })
                        .children(self.children),
                )
                .child(WindowControls {
                    on_close_window: self.on_close_window,
                }),
        )
    }
}

/// A TitleBar Element that can be move the window.
pub struct TitleBarElement {}

impl IntoElement for TitleBarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TitleBarElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let style = Style {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            size: gpui::Size {
                width: relative(1.).into(),
                height: relative(1.).into(),
            },
            ..Default::default()
        };

        let id = window.request_layout(style, [], cx);

        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        use gpui::{MouseButton, MouseMoveEvent, MouseUpEvent};

        window.on_mouse_event(
            move |ev: &MouseMoveEvent, _, window: &mut Window, _cx: &mut App| {
                if bounds.contains(&ev.position) && ev.pressed_button == Some(MouseButton::Left) {
                    window.start_window_move();
                }
            },
        );

        window.on_mouse_event(
            move |ev: &MouseUpEvent, _, window: &mut Window, _cx: &mut App| {
                if ev.button == MouseButton::Left {
                    window.show_window_menu(ev.position);
                }
            },
        );
    }
}
