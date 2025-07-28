use std::mem;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, Context, Decorations, Hsla, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement, Pixels, Render, StatefulInteractiveElement as _, Styled, Window,
    WindowControlArea,
};
use smallvec::{smallvec, SmallVec};
use theme::platform_kind::PlatformKind;
use theme::{ActiveTheme, CLIENT_SIDE_DECORATION_ROUNDING};
use ui::h_flex;

#[cfg(target_os = "linux")]
use crate::platforms::linux::LinuxWindowControls;
use crate::platforms::windows::WindowsWindowControls;

mod platforms;

pub struct TitleBar {
    children: SmallVec<[AnyElement; 2]>,
    should_move: bool,
}

impl Default for TitleBar {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleBar {
    pub fn new() -> Self {
        Self {
            children: smallvec![],
            should_move: false,
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn height(window: &mut Window) -> Pixels {
        (1.75 * window.rem_size()).max(px(34.))
    }

    #[cfg(target_os = "windows")]
    pub fn height(_window: &mut Window) -> Pixels {
        px(32.)
    }

    pub fn title_bar_color(&self, window: &mut Window, cx: &mut Context<Self>) -> Hsla {
        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            if window.is_window_active() && !self.should_move {
                cx.theme().title_bar
            } else {
                cx.theme().title_bar_inactive
            }
        } else {
            cx.theme().title_bar
        }
    }

    pub fn set_children<T>(&mut self, children: T)
    where
        T: IntoIterator<Item = AnyElement>,
    {
        self.children = children.into_iter().collect();
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let supported_controls = window.window_controls();
        let decorations = window.window_decorations();
        let height = Self::height(window);
        let color = self.title_bar_color(window, cx);
        let children = mem::take(&mut self.children);

        h_flex()
            .window_control_area(WindowControlArea::Drag)
            .w_full()
            .h(height)
            .map(|this| {
                if window.is_fullscreen() {
                    this.pl_2()
                } else if cx.theme().platform_kind.is_mac() {
                    this.pl(px(platforms::mac::TRAFFIC_LIGHT_PADDING))
                } else {
                    this.pl_2()
                }
            })
            .map(|this| match decorations {
                Decorations::Server => this,
                Decorations::Client { tiling, .. } => this
                    .when(!(tiling.top || tiling.right), |el| {
                        el.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.top || tiling.left), |el| {
                        el.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    // this border is to avoid a transparent gap in the rounded corners
                    .mt(px(-1.))
                    .border(px(1.))
                    .border_color(color),
            })
            .bg(color)
            .content_stretch()
            .child(
                div()
                    .id("title-bar")
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .when(cx.theme().platform_kind.is_mac(), |this| {
                        this.on_click(|event, window, _| {
                            if event.up.click_count == 2 {
                                window.titlebar_double_click();
                            }
                        })
                    })
                    .when(cx.theme().platform_kind.is_linux(), |this| {
                        this.on_click(|event, window, _| {
                            if event.up.click_count == 2 {
                                window.zoom_window();
                            }
                        })
                    })
                    .children(children),
            )
            .when(!window.is_fullscreen(), |this| {
                match cx.theme().platform_kind {
                    PlatformKind::Linux => {
                        if matches!(decorations, Decorations::Client { .. }) {
                            this.child(LinuxWindowControls::new(None))
                                .when(supported_controls.window_menu, |this| {
                                    this.on_mouse_down(MouseButton::Right, move |ev, window, _| {
                                        window.show_window_menu(ev.position)
                                    })
                                })
                                .on_mouse_move(cx.listener(move |this, _ev, window, _| {
                                    if this.should_move {
                                        this.should_move = false;
                                        window.start_window_move();
                                    }
                                }))
                                .on_mouse_down_out(cx.listener(move |this, _ev, _window, _cx| {
                                    this.should_move = false;
                                }))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(move |this, _ev, _window, _cx| {
                                        this.should_move = false;
                                    }),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _ev, _window, _cx| {
                                        this.should_move = true;
                                    }),
                                )
                        } else {
                            this
                        }
                    }
                    PlatformKind::Windows => this.child(WindowsWindowControls::new(height)),
                    PlatformKind::Mac => this,
                }
            })
    }
}
