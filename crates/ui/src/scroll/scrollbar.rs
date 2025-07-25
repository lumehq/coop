use std::cell::Cell;
use std::ops::Deref;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    fill, point, px, relative, size, App, Axis, BorderStyle, Bounds, ContentMask, Corner,
    CursorStyle, Edges, Element, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId,
    IntoElement, LayoutId, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    Position, ScrollHandle, ScrollWheelEvent, Size, UniformListScrollHandle, Window,
};
use theme::{ActiveTheme, ScrollBarMode};

use crate::AxisExt;

const WIDTH: Pixels = px(2. * 2. + 8.);
const MIN_THUMB_SIZE: f32 = 48.;

const THUMB_WIDTH: Pixels = px(6.);
const THUMB_RADIUS: Pixels = Pixels(6. / 2.);
const THUMB_INSET: Pixels = Pixels(2.);

const THUMB_ACTIVE_WIDTH: Pixels = px(8.);
const THUMB_ACTIVE_RADIUS: Pixels = Pixels(8. / 2.);
const THUMB_ACTIVE_INSET: Pixels = Pixels(2.);

const FADE_OUT_DURATION: f32 = 3.0;
const FADE_OUT_DELAY: f32 = 2.0;

pub trait ScrollHandleOffsetable {
    fn offset(&self) -> Point<Pixels>;
    fn set_offset(&self, offset: Point<Pixels>);
    fn is_uniform_list(&self) -> bool {
        false
    }
    /// The full size of the content, including padding.
    fn content_size(&self) -> Size<Pixels>;
}

impl ScrollHandleOffsetable for ScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.set_offset(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        self.max_offset() + self.bounds().size
    }
}

impl ScrollHandleOffsetable for UniformListScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.0.borrow().base_handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.0.borrow_mut().base_handle.set_offset(offset)
    }

    fn is_uniform_list(&self) -> bool {
        true
    }

    fn content_size(&self) -> Size<Pixels> {
        let base_handle = &self.0.borrow().base_handle;
        base_handle.max_offset() + base_handle.bounds().size
    }
}

#[derive(Debug, Clone)]
pub struct ScrollbarState(Rc<Cell<ScrollbarStateInner>>);

#[derive(Debug, Clone, Copy)]
pub struct ScrollbarStateInner {
    hovered_axis: Option<Axis>,
    hovered_on_thumb: Option<Axis>,
    dragged_axis: Option<Axis>,
    drag_pos: Point<Pixels>,
    last_scroll_offset: Point<Pixels>,
    last_scroll_time: Option<Instant>,
    // Last update offset
    last_update: Instant,
}

impl Default for ScrollbarState {
    fn default() -> Self {
        Self(Rc::new(Cell::new(ScrollbarStateInner {
            hovered_axis: None,
            hovered_on_thumb: None,
            dragged_axis: None,
            drag_pos: point(px(0.), px(0.)),
            last_scroll_offset: point(px(0.), px(0.)),
            last_scroll_time: None,
            last_update: Instant::now(),
        })))
    }
}

impl Deref for ScrollbarState {
    type Target = Rc<Cell<ScrollbarStateInner>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ScrollbarStateInner {
    fn with_drag_pos(&self, axis: Axis, pos: Point<Pixels>) -> Self {
        let mut state = *self;
        if axis.is_vertical() {
            state.drag_pos.y = pos.y;
        } else {
            state.drag_pos.x = pos.x;
        }

        state.dragged_axis = Some(axis);
        state
    }

    fn with_unset_drag_pos(&self) -> Self {
        let mut state = *self;
        state.dragged_axis = None;
        state
    }

    fn with_hovered(&self, axis: Option<Axis>) -> Self {
        let mut state = *self;
        state.hovered_axis = axis;
        if axis.is_some() {
            state.last_scroll_time = Some(std::time::Instant::now());
        }
        state
    }

    fn with_hovered_on_thumb(&self, axis: Option<Axis>) -> Self {
        let mut state = *self;
        state.hovered_on_thumb = axis;
        if self.is_scrollbar_visible() && axis.is_some() {
            state.last_scroll_time = Some(std::time::Instant::now());
        }
        state
    }

    fn with_last_scroll(
        &self,
        last_scroll_offset: Point<Pixels>,
        last_scroll_time: Option<Instant>,
    ) -> Self {
        let mut state = *self;
        state.last_scroll_offset = last_scroll_offset;
        state.last_scroll_time = last_scroll_time;
        state
    }

    fn with_last_scroll_time(&self, t: Option<Instant>) -> Self {
        let mut state = *self;
        state.last_scroll_time = t;
        state
    }

    fn with_last_update(&self, t: Instant) -> Self {
        let mut state = *self;
        state.last_update = t;
        state
    }

    fn is_scrollbar_visible(&self) -> bool {
        // On drag
        if self.dragged_axis.is_some() {
            return true;
        }

        if let Some(last_time) = self.last_scroll_time {
            let elapsed = Instant::now().duration_since(last_time).as_secs_f32();
            elapsed < FADE_OUT_DURATION
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarAxis {
    Vertical,
    Horizontal,
    Both,
}

impl From<Axis> for ScrollbarAxis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::Vertical => Self::Vertical,
            Axis::Horizontal => Self::Horizontal,
        }
    }
}

impl ScrollbarAxis {
    /// Return true if the scrollbar axis is vertical.
    pub fn is_vertical(&self) -> bool {
        matches!(self, Self::Vertical)
    }

    /// Return true if the scrollbar axis is horizontal.
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Self::Horizontal)
    }

    /// Return true if the scrollbar axis is both vertical and horizontal.
    pub fn is_both(&self) -> bool {
        matches!(self, Self::Both)
    }

    #[inline]
    pub fn has_vertical(&self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }

    #[inline]
    pub fn has_horizontal(&self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }

    #[inline]
    fn all(&self) -> Vec<Axis> {
        match self {
            Self::Vertical => vec![Axis::Vertical],
            Self::Horizontal => vec![Axis::Horizontal],
            // This should keep Horizontal first, Vertical is the primary axis
            // if Vertical not need display, then Horizontal will not keep right margin.
            Self::Both => vec![Axis::Horizontal, Axis::Vertical],
        }
    }
}

/// Scrollbar control for scroll-area or a uniform-list.
pub struct Scrollbar {
    axis: ScrollbarAxis,
    scroll_handle: Rc<Box<dyn ScrollHandleOffsetable>>,
    state: ScrollbarState,
    scroll_size: Option<Size<Pixels>>,
    /// Maximum frames per second for scrolling by drag. Default is 120 FPS.
    ///
    /// This is used to limit the update rate of the scrollbar when it is
    /// being dragged for some complex interactions for reducing CPU usage.
    max_fps: usize,
}

impl Scrollbar {
    fn new(
        axis: impl Into<ScrollbarAxis>,
        state: &ScrollbarState,
        scroll_handle: &(impl ScrollHandleOffsetable + Clone + 'static),
    ) -> Self {
        Self {
            state: state.clone(),
            axis: axis.into(),
            scroll_handle: Rc::new(Box::new(scroll_handle.clone())),
            max_fps: 120,
            scroll_size: None,
        }
    }

    /// Create with vertical and horizontal scrollbar.
    pub fn both(
        state: &ScrollbarState,
        scroll_handle: &(impl ScrollHandleOffsetable + Clone + 'static),
    ) -> Self {
        Self::new(ScrollbarAxis::Both, state, scroll_handle)
    }

    /// Create with horizontal scrollbar.
    pub fn horizontal(
        state: &ScrollbarState,
        scroll_handle: &(impl ScrollHandleOffsetable + Clone + 'static),
    ) -> Self {
        Self::new(ScrollbarAxis::Horizontal, state, scroll_handle)
    }

    /// Create with vertical scrollbar.
    pub fn vertical(
        state: &ScrollbarState,
        scroll_handle: &(impl ScrollHandleOffsetable + Clone + 'static),
    ) -> Self {
        Self::new(ScrollbarAxis::Vertical, state, scroll_handle)
    }

    /// Create vertical scrollbar for uniform list.
    pub fn uniform_scroll(
        state: &ScrollbarState,
        scroll_handle: &(impl ScrollHandleOffsetable + Clone + 'static),
    ) -> Self {
        Self::new(ScrollbarAxis::Vertical, state, scroll_handle)
    }

    /// Set a special scroll size of the content area, default is None.
    ///
    /// Default will sync the `content_size` from `scroll_handle`.
    pub fn scroll_size(mut self, scroll_size: Size<Pixels>) -> Self {
        self.scroll_size = Some(scroll_size);
        self
    }

    /// Set scrollbar axis.
    pub fn axis(mut self, axis: impl Into<ScrollbarAxis>) -> Self {
        self.axis = axis.into();
        self
    }

    /// Set maximum frames per second for scrolling by drag. Default is 120 FPS.
    ///
    /// If you have very high CPU usage, consider reducing this value to improve performance.
    ///
    /// Available values: 30..120
    pub fn max_fps(mut self, max_fps: usize) -> Self {
        self.max_fps = max_fps.clamp(30, 120);
        self
    }

    fn style_for_active(cx: &App) -> (Hsla, Hsla, Hsla, Pixels, Pixels, Pixels) {
        (
            cx.theme().scrollbar_thumb_hover_background,
            cx.theme().scrollbar_thumb_background,
            cx.theme().scrollbar_thumb_border,
            THUMB_ACTIVE_WIDTH,
            THUMB_ACTIVE_INSET,
            THUMB_ACTIVE_RADIUS,
        )
    }

    fn style_for_hovered_thumb(cx: &App) -> (Hsla, Hsla, Hsla, Pixels, Pixels, Pixels) {
        (
            cx.theme().scrollbar_thumb_hover_background,
            cx.theme().scrollbar_thumb_background,
            cx.theme().scrollbar_thumb_border,
            THUMB_ACTIVE_WIDTH,
            THUMB_ACTIVE_INSET,
            THUMB_ACTIVE_RADIUS,
        )
    }

    fn style_for_hovered_bar(cx: &App) -> (Hsla, Hsla, Hsla, Pixels, Pixels, Pixels) {
        (
            cx.theme().scrollbar_thumb_background,
            cx.theme().scrollbar_thumb_border,
            gpui::transparent_black(),
            THUMB_ACTIVE_WIDTH,
            THUMB_ACTIVE_INSET,
            THUMB_ACTIVE_RADIUS,
        )
    }

    fn style_for_idle(cx: &App) -> (Hsla, Hsla, Hsla, Pixels, Pixels, Pixels) {
        let (width, inset, radius) = match cx.theme().scrollbar_mode {
            ScrollBarMode::Scrolling => (THUMB_WIDTH, THUMB_INSET, THUMB_RADIUS),
            _ => (THUMB_ACTIVE_WIDTH, THUMB_ACTIVE_INSET, THUMB_ACTIVE_RADIUS),
        };

        (
            gpui::transparent_black(),
            gpui::transparent_black(),
            gpui::transparent_black(),
            width,
            inset,
            radius,
        )
    }
}

impl IntoElement for Scrollbar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct PrepaintState {
    hitbox: Hitbox,
    states: Vec<AxisPrepaintState>,
}

pub struct AxisPrepaintState {
    axis: Axis,
    bar_hitbox: Hitbox,
    bounds: Bounds<Pixels>,
    radius: Pixels,
    bg: Hsla,
    border: Hsla,
    thumb_bounds: Bounds<Pixels>,
    // Bounds of thumb to be rendered.
    thumb_fill_bounds: Bounds<Pixels>,
    thumb_bg: Hsla,
    scroll_size: Pixels,
    container_size: Pixels,
    thumb_size: Pixels,
    margin_end: Pixels,
}

impl Element for Scrollbar {
    type PrepaintState = PrepaintState;
    type RequestLayoutState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = gpui::Style {
            position: Position::Absolute,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            size: gpui::Size {
                width: relative(1.).into(),
                height: relative(1.).into(),
            },
            ..Default::default()
        };

        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        });

        let mut states = vec![];
        let mut has_both = self.axis.is_both();
        let scroll_size = self
            .scroll_size
            .unwrap_or(self.scroll_handle.content_size());

        for axis in self.axis.all().into_iter() {
            let is_vertical = axis.is_vertical();
            let (scroll_area_size, container_size, scroll_position) = if is_vertical {
                (
                    scroll_size.height,
                    hitbox.size.height,
                    self.scroll_handle.offset().y,
                )
            } else {
                (
                    scroll_size.width,
                    hitbox.size.width,
                    self.scroll_handle.offset().x,
                )
            };

            // The horizontal scrollbar is set avoid overlapping with the vertical scrollbar, if the vertical scrollbar is visible.

            let margin_end = if has_both && !is_vertical {
                THUMB_ACTIVE_WIDTH
            } else {
                px(0.)
            };

            // Hide scrollbar, if the scroll area is smaller than the container.
            if scroll_area_size <= container_size {
                has_both = false;
                continue;
            }

            let thumb_length =
                (container_size / scroll_area_size * container_size).max(px(MIN_THUMB_SIZE));
            let thumb_start = -(scroll_position / (scroll_area_size - container_size)
                * (container_size - margin_end - thumb_length));
            let thumb_end = (thumb_start + thumb_length).min(container_size - margin_end);

            let bounds = Bounds {
                origin: if is_vertical {
                    point(hitbox.origin.x + hitbox.size.width - WIDTH, hitbox.origin.y)
                } else {
                    point(
                        hitbox.origin.x,
                        hitbox.origin.y + hitbox.size.height - WIDTH,
                    )
                },
                size: gpui::Size {
                    width: if is_vertical {
                        WIDTH
                    } else {
                        hitbox.size.width
                    },
                    height: if is_vertical {
                        hitbox.size.height
                    } else {
                        WIDTH
                    },
                },
            };

            let state = self.state.clone();
            let is_always_to_show = cx.theme().scrollbar_mode.is_always();
            let is_hover_to_show = cx.theme().scrollbar_mode.is_hover();
            let is_hovered_on_bar = state.get().hovered_axis == Some(axis);
            let is_hovered_on_thumb = state.get().hovered_on_thumb == Some(axis);

            let (thumb_bg, bar_bg, bar_border, thumb_width, inset, radius) =
                if state.get().dragged_axis == Some(axis) {
                    Self::style_for_active(cx)
                } else if is_hover_to_show && (is_hovered_on_bar || is_hovered_on_thumb) {
                    if is_hovered_on_thumb {
                        Self::style_for_hovered_thumb(cx)
                    } else {
                        Self::style_for_hovered_bar(cx)
                    }
                } else if is_always_to_show {
                    #[allow(clippy::if_same_then_else)]
                    if is_hovered_on_thumb {
                        Self::style_for_hovered_thumb(cx)
                    } else {
                        Self::style_for_hovered_bar(cx)
                    }
                } else {
                    let mut idle_state = Self::style_for_idle(cx);
                    // Delay 2s to fade out the scrollbar thumb (in 1s)
                    if let Some(last_time) = state.get().last_scroll_time {
                        let elapsed = Instant::now().duration_since(last_time).as_secs_f32();
                        if elapsed < FADE_OUT_DURATION {
                            if is_hovered_on_bar {
                                state.set(state.get().with_last_scroll_time(Some(Instant::now())));
                                idle_state = if is_hovered_on_thumb {
                                    Self::style_for_hovered_thumb(cx)
                                } else {
                                    Self::style_for_hovered_bar(cx)
                                };
                            } else {
                                if elapsed < FADE_OUT_DELAY {
                                    idle_state.0 = cx.theme().scrollbar_thumb_background;
                                } else {
                                    // opacity = 1 - (x - 2)^10
                                    let opacity = 1.0 - (elapsed - FADE_OUT_DELAY).powi(10);
                                    idle_state.0 =
                                        cx.theme().scrollbar_thumb_background.opacity(opacity);
                                };

                                window.request_animation_frame();
                            }
                        }
                    }

                    idle_state
                };

            // The clickable area of the thumb
            let thumb_length = thumb_end - thumb_start - inset * 2;
            let thumb_bounds = if is_vertical {
                Bounds::from_corner_and_size(
                    Corner::TopRight,
                    bounds.top_right() + point(-inset, inset + thumb_start),
                    size(WIDTH, thumb_length),
                )
            } else {
                Bounds::from_corner_and_size(
                    Corner::BottomLeft,
                    bounds.bottom_left() + point(inset + thumb_start, -inset),
                    size(thumb_length, WIDTH),
                )
            };

            // The actual render area of the thumb
            let thumb_fill_bounds = if is_vertical {
                Bounds::from_corner_and_size(
                    Corner::TopRight,
                    bounds.top_right() + point(-inset, inset + thumb_start),
                    size(thumb_width, thumb_length),
                )
            } else {
                Bounds::from_corner_and_size(
                    Corner::BottomLeft,
                    bounds.bottom_left() + point(inset + thumb_start, -inset),
                    size(thumb_length, thumb_width),
                )
            };

            let bar_hitbox = window.with_content_mask(Some(ContentMask { bounds }), |window| {
                window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
            });

            states.push(AxisPrepaintState {
                axis,
                bar_hitbox,
                bounds,
                radius,
                bg: bar_bg,
                border: bar_border,
                thumb_bounds,
                thumb_fill_bounds,
                thumb_bg,
                scroll_size: scroll_area_size,
                container_size,
                thumb_size: thumb_length,
                margin_end,
            })
        }

        PrepaintState { hitbox, states }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view_id = window.current_view();
        let hitbox_bounds = prepaint.hitbox.bounds;
        let is_visible =
            self.state.get().is_scrollbar_visible() || cx.theme().scrollbar_mode.is_always();
        let is_hover_to_show = cx.theme().scrollbar_mode.is_hover();

        // Update last_scroll_time when offset is changed.
        if self.scroll_handle.offset() != self.state.get().last_scroll_offset {
            self.state.set(
                self.state
                    .get()
                    .with_last_scroll(self.scroll_handle.offset(), Some(Instant::now())),
            );
        }

        window.with_content_mask(
            Some(ContentMask {
                bounds: hitbox_bounds,
            }),
            |window| {
                for state in prepaint.states.iter() {
                    let axis = state.axis;
                    let radius = state.radius;
                    let bounds = state.bounds;
                    let thumb_bounds = state.thumb_bounds;
                    let scroll_area_size = state.scroll_size;
                    let container_size = state.container_size;
                    let thumb_size = state.thumb_size;
                    let margin_end = state.margin_end;
                    let is_vertical = axis.is_vertical();

                    window.set_cursor_style(CursorStyle::default(), &state.bar_hitbox);

                    window.paint_layer(hitbox_bounds, |cx| {
                        cx.paint_quad(fill(state.bounds, state.bg));

                        cx.paint_quad(PaintQuad {
                            bounds,
                            corner_radii: (0.).into(),
                            background: gpui::transparent_black().into(),
                            border_widths: Edges {
                                top: px(0.),
                                right: px(0.),
                                bottom: px(0.),
                                left: px(0.),
                            },
                            border_color: state.border,
                            border_style: BorderStyle::default(),
                        });

                        cx.paint_quad(
                            fill(state.thumb_fill_bounds, state.thumb_bg).corner_radii(radius),
                        );
                    });

                    window.on_mouse_event({
                        let state = self.state.clone();
                        let scroll_handle = self.scroll_handle.clone();

                        move |event: &ScrollWheelEvent, phase, _, cx| {
                            if phase.bubble()
                                && hitbox_bounds.contains(&event.position)
                                && scroll_handle.offset() != state.get().last_scroll_offset
                            {
                                state.set(state.get().with_last_scroll(
                                    scroll_handle.offset(),
                                    Some(Instant::now()),
                                ));
                                cx.notify(view_id);
                            }
                        }
                    });

                    let safe_range = (-scroll_area_size + container_size)..px(0.);

                    if is_hover_to_show || is_visible {
                        window.on_mouse_event({
                            let state = self.state.clone();
                            let scroll_handle = self.scroll_handle.clone();

                            move |event: &MouseDownEvent, phase, _, cx| {
                                if phase.bubble() && bounds.contains(&event.position) {
                                    cx.stop_propagation();

                                    if thumb_bounds.contains(&event.position) {
                                        // click on the thumb bar, set the drag position
                                        let pos = event.position - thumb_bounds.origin;

                                        state.set(state.get().with_drag_pos(axis, pos));

                                        cx.notify(view_id);
                                    } else {
                                        // click on the scrollbar, jump to the position
                                        // Set the thumb bar center to the click position
                                        let offset = scroll_handle.offset();
                                        let percentage = if is_vertical {
                                            (event.position.y - thumb_size / 2. - bounds.origin.y)
                                                / (bounds.size.height - thumb_size)
                                        } else {
                                            (event.position.x - thumb_size / 2. - bounds.origin.x)
                                                / (bounds.size.width - thumb_size)
                                        }
                                        .min(1.);

                                        if is_vertical {
                                            scroll_handle.set_offset(point(
                                                offset.x,
                                                (-scroll_area_size * percentage)
                                                    .clamp(safe_range.start, safe_range.end),
                                            ));
                                        } else {
                                            scroll_handle.set_offset(point(
                                                (-scroll_area_size * percentage)
                                                    .clamp(safe_range.start, safe_range.end),
                                                offset.y,
                                            ));
                                        }
                                    }
                                }
                            }
                        });
                    }

                    window.on_mouse_event({
                        let scroll_handle = self.scroll_handle.clone();
                        let state = self.state.clone();
                        let max_fps_duration = Duration::from_millis((1000 / self.max_fps) as u64);

                        move |event: &MouseMoveEvent, _, _, cx| {
                            let mut notify = false;
                            // When is hover to show mode or it was visible,
                            // we need to update the hovered state and increase the last_scroll_time.
                            let need_hover_to_update = is_hover_to_show || is_visible;
                            // Update hovered state for scrollbar
                            if bounds.contains(&event.position) && need_hover_to_update {
                                state.set(state.get().with_hovered(Some(axis)));

                                if state.get().hovered_axis != Some(axis) {
                                    notify = true;
                                }
                            } else if state.get().hovered_axis == Some(axis)
                                && state.get().hovered_axis.is_some()
                            {
                                state.set(state.get().with_hovered(None));
                                notify = true;
                            }

                            // Update hovered state for scrollbar thumb
                            if thumb_bounds.contains(&event.position) {
                                if state.get().hovered_on_thumb != Some(axis) {
                                    state.set(state.get().with_hovered_on_thumb(Some(axis)));
                                    notify = true;
                                }
                            } else if state.get().hovered_on_thumb == Some(axis) {
                                state.set(state.get().with_hovered_on_thumb(None));
                                notify = true;
                            }

                            // Move thumb position on dragging
                            if state.get().dragged_axis == Some(axis) && event.dragging() {
                                // drag_pos is the position of the mouse down event
                                // We need to keep the thumb bar still at the origin down position
                                let drag_pos = state.get().drag_pos;

                                let percentage = (if is_vertical {
                                    (event.position.y - drag_pos.y - bounds.origin.y)
                                        / (bounds.size.height - thumb_size)
                                } else {
                                    (event.position.x - drag_pos.x - bounds.origin.x)
                                        / (bounds.size.width - thumb_size - margin_end)
                                })
                                .clamp(0., 1.);

                                let offset = if is_vertical {
                                    point(
                                        scroll_handle.offset().x,
                                        (-(scroll_area_size - container_size) * percentage)
                                            .clamp(safe_range.start, safe_range.end),
                                    )
                                } else {
                                    point(
                                        (-(scroll_area_size - container_size) * percentage)
                                            .clamp(safe_range.start, safe_range.end),
                                        scroll_handle.offset().y,
                                    )
                                };

                                if (scroll_handle.offset().y - offset.y).abs() > px(1.)
                                    || (scroll_handle.offset().x - offset.x).abs() > px(1.)
                                {
                                    // Limit update rate
                                    if state.get().last_update.elapsed() > max_fps_duration {
                                        scroll_handle.set_offset(offset);
                                        state.set(state.get().with_last_update(Instant::now()));
                                        notify = true;
                                    }
                                }
                            }

                            if notify {
                                cx.notify(view_id);
                            }
                        }
                    });

                    window.on_mouse_event({
                        let state = self.state.clone();

                        move |_event: &MouseUpEvent, phase, _, cx| {
                            if phase.bubble() {
                                state.set(state.get().with_unset_drag_pos());
                                cx.notify(view_id);
                            }
                        }
                    });
                }
            },
        );
    }
}
