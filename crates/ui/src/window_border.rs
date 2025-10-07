use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, div, point, px, AnyElement, App, Bounds, CursorStyle, Decorations, Edges,
    HitboxBehavior, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels,
    Point, RenderOnce, ResizeEdge, Size, Styled as _, Window,
};
use theme::{CLIENT_SIDE_DECORATION_ROUNDING, CLIENT_SIDE_DECORATION_SHADOW};

const WINDOW_BORDER_WIDTH: Pixels = px(1.0);

/// Create a new window border.
pub fn window_border() -> WindowBorder {
    WindowBorder::new()
}

/// Window border use to render a custom window border and shadow for Linux.
#[derive(IntoElement, Default)]
pub struct WindowBorder {
    children: Vec<AnyElement>,
}

/// Get the window paddings.
pub fn window_paddings(window: &Window, _cx: &App) -> Edges<Pixels> {
    match window.window_decorations() {
        Decorations::Server => Edges::all(px(0.0)),
        Decorations::Client { tiling } => {
            let mut paddings = Edges::all(CLIENT_SIDE_DECORATION_SHADOW);
            if tiling.top {
                paddings.top = px(0.0);
            }
            if tiling.bottom {
                paddings.bottom = px(0.0);
            }
            if tiling.left {
                paddings.left = px(0.0);
            }
            if tiling.right {
                paddings.right = px(0.0);
            }
            paddings
        }
    }
}

impl WindowBorder {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}

impl ParentElement for WindowBorder {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for WindowBorder {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let decorations = window.window_decorations();
        window.set_client_inset(CLIENT_SIDE_DECORATION_SHADOW);

        div()
            .id("window-backdrop")
            .bg(gpui::transparent_black())
            .map(|div| match decorations {
                Decorations::Server => div,
                Decorations::Client { tiling, .. } => div
                    .bg(gpui::transparent_black())
                    .child(
                        canvas(
                            |_bounds, window, _cx| {
                                window.insert_hitbox(
                                    Bounds::new(
                                        point(px(0.0), px(0.0)),
                                        window.window_bounds().get_bounds().size,
                                    ),
                                    HitboxBehavior::Normal,
                                )
                            },
                            move |_bounds, hitbox, window, _cx| {
                                let mouse = window.mouse_position();
                                let size = window.window_bounds().get_bounds().size;
                                let Some(edge) =
                                    resize_edge(mouse, CLIENT_SIDE_DECORATION_SHADOW, size)
                                else {
                                    return;
                                };
                                window.set_cursor_style(
                                    match edge {
                                        ResizeEdge::Top | ResizeEdge::Bottom => {
                                            CursorStyle::ResizeUpDown
                                        }
                                        ResizeEdge::Left | ResizeEdge::Right => {
                                            CursorStyle::ResizeLeftRight
                                        }
                                        ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                            CursorStyle::ResizeUpLeftDownRight
                                        }
                                        ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                            CursorStyle::ResizeUpRightDownLeft
                                        }
                                    },
                                    &hitbox,
                                );
                            },
                        )
                        .size_full()
                        .absolute(),
                    )
                    .when(!(tiling.top || tiling.right), |div| {
                        div.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.top || tiling.left), |div| {
                        div.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.right), |div| {
                        div.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.left), |div| {
                        div.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!tiling.top, |div| div.pt(CLIENT_SIDE_DECORATION_SHADOW))
                    .when(!tiling.bottom, |div| div.pb(CLIENT_SIDE_DECORATION_SHADOW))
                    .when(!tiling.left, |div| div.pl(CLIENT_SIDE_DECORATION_SHADOW))
                    .when(!tiling.right, |div| div.pr(CLIENT_SIDE_DECORATION_SHADOW))
                    .on_mouse_down(MouseButton::Left, move |_, window, _cx| {
                        let size = window.window_bounds().get_bounds().size;
                        let pos = window.mouse_position();

                        if let Some(edge) = resize_edge(pos, CLIENT_SIDE_DECORATION_SHADOW, size) {
                            window.start_window_resize(edge)
                        };
                    }),
            })
            .size_full()
            .child(
                div()
                    .map(|div| match decorations {
                        Decorations::Server => div,
                        Decorations::Client { tiling } => div
                            .when(!(tiling.top || tiling.right), |div| {
                                div.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.top || tiling.left), |div| {
                                div.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.right), |div| {
                                div.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.left), |div| {
                                div.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!tiling.top, |div| div.border_t(WINDOW_BORDER_WIDTH))
                            .when(!tiling.bottom, |div| div.border_b(WINDOW_BORDER_WIDTH))
                            .when(!tiling.left, |div| div.border_l(WINDOW_BORDER_WIDTH))
                            .when(!tiling.right, |div| div.border_r(WINDOW_BORDER_WIDTH))
                            .when(!tiling.is_tiled(), |div| {
                                div.shadow(vec![gpui::BoxShadow {
                                    color: Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.3,
                                    },
                                    blur_radius: CLIENT_SIDE_DECORATION_SHADOW / 2.,
                                    spread_radius: px(0.),
                                    offset: point(px(0.0), px(0.0)),
                                }])
                            }),
                    })
                    .on_mouse_move(|_e, _window, cx| {
                        cx.stop_propagation();
                    })
                    .bg(gpui::transparent_black())
                    .size_full()
                    .children(self.children),
            )
    }
}

fn resize_edge(pos: Point<Pixels>, shadow_size: Pixels, size: Size<Pixels>) -> Option<ResizeEdge> {
    let edge = if pos.y < shadow_size && pos.x < shadow_size {
        ResizeEdge::TopLeft
    } else if pos.y < shadow_size && pos.x > size.width - shadow_size {
        ResizeEdge::TopRight
    } else if pos.y < shadow_size {
        ResizeEdge::Top
    } else if pos.y > size.height - shadow_size && pos.x < shadow_size {
        ResizeEdge::BottomLeft
    } else if pos.y > size.height - shadow_size && pos.x > size.width - shadow_size {
        ResizeEdge::BottomRight
    } else if pos.y > size.height - shadow_size {
        ResizeEdge::Bottom
    } else if pos.x < shadow_size {
        ResizeEdge::Left
    } else if pos.x > size.width - shadow_size {
        ResizeEdge::Right
    } else {
        return None;
    };
    Some(edge)
}
