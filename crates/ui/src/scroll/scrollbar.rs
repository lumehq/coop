use gpui::*;
use serde::{Deserialize, Serialize};
use std::{cell::Cell, rc::Rc, time::Instant};

use crate::theme::ActiveTheme;

/// Scrollbar show mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
pub enum ScrollbarShow {
    #[default]
    Scrolling,
    Hover,
}

impl ScrollbarShow {
    fn is_hover(&self) -> bool {
        matches!(self, Self::Hover)
    }
}

const BORDER_WIDTH: Pixels = px(0.);
const MIN_THUMB_SIZE: f32 = 80.;
const THUMB_RADIUS: Pixels = Pixels(3.0);
const THUMB_INSET: Pixels = Pixels(4.);
const FADE_OUT_DURATION: f32 = 3.0;
const FADE_OUT_DELAY: f32 = 2.0;

pub trait ScrollHandleOffsetable {
    fn offset(&self) -> Point<Pixels>;
    fn set_offset(&self, offset: Point<Pixels>);
    fn is_uniform_list(&self) -> bool {
        false
    }
}

impl ScrollHandleOffsetable for ScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.set_offset(offset);
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
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollbarState {
    hovered_axis: Option<ScrollbarAxis>,
    hovered_on_thumb: Option<ScrollbarAxis>,
    dragged_axis: Option<ScrollbarAxis>,
    drag_pos: Point<Pixels>,
    last_scroll_offset: Point<Pixels>,
    last_scroll_time: Option<Instant>,
}

impl Default for ScrollbarState {
    fn default() -> Self {
        Self {
            hovered_axis: None,
            hovered_on_thumb: None,
            dragged_axis: None,
            drag_pos: point(px(0.), px(0.)),
            last_scroll_offset: point(px(0.), px(0.)),
            last_scroll_time: None,
        }
    }
}

impl ScrollbarState {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_drag_pos(&self, axis: ScrollbarAxis, pos: Point<Pixels>) -> Self {
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

    fn with_hovered(&self, axis: Option<ScrollbarAxis>) -> Self {
        let mut state = *self;
        state.hovered_axis = axis;
        if self.is_scrollbar_visible() {
            state.last_scroll_time = Some(Instant::now());
        }
        state
    }

    fn with_hovered_on_thumb(&self, axis: Option<ScrollbarAxis>) -> Self {
        let mut state = *self;
        state.hovered_on_thumb = axis;
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

    fn is_scrollbar_visible(&self) -> bool {
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

impl ScrollbarAxis {
    #[inline]
    fn is_vertical(&self) -> bool {
        matches!(self, Self::Vertical)
    }

    #[inline]
    fn is_both(&self) -> bool {
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
    fn all(&self) -> Vec<ScrollbarAxis> {
        match self {
            Self::Vertical => vec![Self::Vertical],
            Self::Horizontal => vec![Self::Horizontal],
            // This should keep vertical first, vertical is the primary axis
            // if vertical not need display, then horizontal will not keep right margin.
            Self::Both => vec![Self::Vertical, Self::Horizontal],
        }
    }
}

/// Scrollbar control for scroll-area or a uniform-list.
pub struct Scrollbar {
    view_id: EntityId,
    axis: ScrollbarAxis,
    /// When is vertical, this is the height of the scrollbar.
    width: Pixels,
    scroll_handle: Rc<Box<dyn ScrollHandleOffsetable>>,
    scroll_size: gpui::Size<Pixels>,
    state: Rc<Cell<ScrollbarState>>,
}

impl Scrollbar {
    fn new(
        view_id: EntityId,
        state: Rc<Cell<ScrollbarState>>,
        axis: ScrollbarAxis,
        scroll_handle: impl ScrollHandleOffsetable + 'static,
        scroll_size: gpui::Size<Pixels>,
    ) -> Self {
        Self {
            view_id,
            state,
            axis,
            scroll_size,
            width: px(12.),
            scroll_handle: Rc::new(Box::new(scroll_handle)),
        }
    }

    /// Create with vertical and horizontal scrollbar.
    pub fn both(
        view_id: EntityId,
        state: Rc<Cell<ScrollbarState>>,
        scroll_handle: impl ScrollHandleOffsetable + 'static,
        scroll_size: gpui::Size<Pixels>,
    ) -> Self {
        Self::new(
            view_id,
            state,
            ScrollbarAxis::Both,
            scroll_handle,
            scroll_size,
        )
    }

    /// Create with horizontal scrollbar.
    pub fn horizontal(
        view_id: EntityId,
        state: Rc<Cell<ScrollbarState>>,
        scroll_handle: impl ScrollHandleOffsetable + 'static,
        scroll_size: gpui::Size<Pixels>,
    ) -> Self {
        Self::new(
            view_id,
            state,
            ScrollbarAxis::Horizontal,
            scroll_handle,
            scroll_size,
        )
    }

    /// Create with vertical scrollbar.
    pub fn vertical(
        view_id: EntityId,
        state: Rc<Cell<ScrollbarState>>,
        scroll_handle: impl ScrollHandleOffsetable + 'static,
        scroll_size: gpui::Size<Pixels>,
    ) -> Self {
        Self::new(
            view_id,
            state,
            ScrollbarAxis::Vertical,
            scroll_handle,
            scroll_size,
        )
    }

    /// Create vertical scrollbar for uniform list.
    pub fn uniform_scroll(
        view_id: EntityId,
        state: Rc<Cell<ScrollbarState>>,
        scroll_handle: UniformListScrollHandle,
    ) -> Self {
        let scroll_size = scroll_handle
            .0
            .borrow()
            .last_item_size
            .map(|size| size.contents)
            .unwrap_or_default();

        Self::new(
            view_id,
            state,
            ScrollbarAxis::Vertical,
            scroll_handle,
            scroll_size,
        )
    }

    /// Set scrollbar axis.
    pub fn axis(mut self, axis: ScrollbarAxis) -> Self {
        self.axis = axis;
        self
    }

    fn style_for_active(cx: &AppContext) -> (Hsla, Hsla, Hsla, Pixels, Pixels) {
        (
            cx.theme().scrollbar_thumb_hover,
            cx.theme().scrollbar,
            cx.theme().border,
            THUMB_INSET - px(1.),
            THUMB_RADIUS,
        )
    }

    fn style_for_hovered_thumb(cx: &AppContext) -> (Hsla, Hsla, Hsla, Pixels, Pixels) {
        (
            cx.theme().scrollbar_thumb_hover,
            cx.theme().scrollbar,
            cx.theme().border,
            THUMB_INSET - px(1.),
            THUMB_RADIUS,
        )
    }

    fn style_for_hovered_bar(cx: &AppContext) -> (Hsla, Hsla, Hsla, Pixels, Pixels) {
        let (inset, radius) = if cx.theme().scrollbar_show.is_hover() {
            (THUMB_INSET, THUMB_RADIUS - px(1.))
        } else {
            (THUMB_INSET - px(1.), THUMB_RADIUS)
        };

        (
            cx.theme().scrollbar_thumb,
            cx.theme().scrollbar,
            gpui::transparent_black(),
            inset,
            radius,
        )
    }

    fn style_for_idle(_: &AppContext) -> (Hsla, Hsla, Hsla, Pixels, Pixels) {
        (
            gpui::transparent_black(),
            gpui::transparent_black(),
            gpui::transparent_black(),
            THUMB_INSET,
            THUMB_RADIUS - px(1.),
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
    axis: ScrollbarAxis,
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
    type RequestLayoutState = ();

    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        cx: &mut gpui::WindowContext,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let style = Style {
            position: Position::Absolute,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            size: Size {
                width: relative(1.).into(),
                height: relative(1.).into(),
            },
            ..Default::default()
        };

        (cx.request_layout(style, None), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        cx: &mut gpui::WindowContext,
    ) -> Self::PrepaintState {
        let hitbox = cx.with_content_mask(Some(ContentMask { bounds }), |cx| {
            cx.insert_hitbox(bounds, false)
        });

        let mut states = vec![];

        let mut has_both = self.axis.is_both();

        for axis in self.axis.all().into_iter() {
            let is_vertical = axis.is_vertical();
            let (scroll_area_size, container_size, scroll_position) = if is_vertical {
                (
                    self.scroll_size.height,
                    hitbox.size.height,
                    self.scroll_handle.offset().y,
                )
            } else {
                (
                    self.scroll_size.width,
                    hitbox.size.width,
                    self.scroll_handle.offset().x,
                )
            };

            // The horizontal scrollbar is set avoid overlapping with the vertical scrollbar, if the vertical scrollbar is visible.
            let margin_end = if has_both && !is_vertical {
                self.width
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
                    point(
                        hitbox.origin.x + hitbox.size.width - self.width,
                        hitbox.origin.y,
                    )
                } else {
                    point(
                        hitbox.origin.x,
                        hitbox.origin.y + hitbox.size.height - self.width,
                    )
                },
                size: gpui::Size {
                    width: if is_vertical {
                        self.width
                    } else {
                        hitbox.size.width
                    },
                    height: if is_vertical {
                        hitbox.size.height
                    } else {
                        self.width
                    },
                },
            };

            let state = self.state.clone();
            let is_hover_to_show = cx.theme().scrollbar_show.is_hover();
            let is_hovered_on_bar = state.get().hovered_axis == Some(axis);
            let is_hovered_on_thumb = state.get().hovered_on_thumb == Some(axis);

            let (thumb_bg, bar_bg, bar_border, inset, radius) =
                if state.get().dragged_axis == Some(axis) {
                    Self::style_for_active(cx)
                } else if is_hover_to_show && is_hovered_on_bar {
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
                                    idle_state.0 = cx.theme().scrollbar_thumb;
                                } else {
                                    // opacity = 1 - (x - 2)^10
                                    let opacity = 1.0 - (elapsed - FADE_OUT_DELAY).powi(10);
                                    idle_state.0 = cx.theme().scrollbar_thumb.opacity(opacity);
                                };

                                cx.request_animation_frame();
                            }
                        }
                    }

                    idle_state
                };

            let thumb_bounds = if is_vertical {
                Bounds::from_corners(
                    point(bounds.origin.x, bounds.origin.y + thumb_start),
                    point(bounds.origin.x + self.width, bounds.origin.y + thumb_end),
                )
            } else {
                Bounds::from_corners(
                    point(bounds.origin.x + thumb_start, bounds.origin.y),
                    point(bounds.origin.x + thumb_end, bounds.origin.y + self.width),
                )
            };
            let thumb_fill_bounds = if is_vertical {
                Bounds::from_corners(
                    point(
                        bounds.origin.x + inset + BORDER_WIDTH,
                        bounds.origin.y + thumb_start + inset,
                    ),
                    point(
                        bounds.origin.x + self.width - inset,
                        bounds.origin.y + thumb_end - inset,
                    ),
                )
            } else {
                Bounds::from_corners(
                    point(
                        bounds.origin.x + thumb_start + inset,
                        bounds.origin.y + inset + BORDER_WIDTH,
                    ),
                    point(
                        bounds.origin.x + thumb_end - inset,
                        bounds.origin.y + self.width - inset,
                    ),
                )
            };

            let bar_hitbox = cx.with_content_mask(Some(ContentMask { bounds }), |cx| {
                cx.insert_hitbox(bounds, false)
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
        _: Option<&gpui::GlobalElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        cx: &mut gpui::WindowContext,
    ) {
        let hitbox_bounds = prepaint.hitbox.bounds;
        let is_visible = self.state.get().is_scrollbar_visible();
        let is_hover_to_show = cx.theme().scrollbar_show.is_hover();

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

            cx.set_cursor_style(CursorStyle::default(), &state.bar_hitbox);

            cx.paint_layer(hitbox_bounds, |cx| {
                cx.paint_quad(fill(state.bounds, state.bg));

                cx.paint_quad(PaintQuad {
                    bounds,
                    corner_radii: (0.).into(),
                    background: gpui::transparent_black().into(),
                    border_widths: if is_vertical {
                        Edges {
                            top: px(0.),
                            right: px(0.),
                            bottom: px(0.),
                            left: BORDER_WIDTH,
                        }
                    } else {
                        Edges {
                            top: BORDER_WIDTH,
                            right: px(0.),
                            bottom: px(0.),
                            left: px(0.),
                        }
                    },
                    border_color: state.border,
                });

                cx.paint_quad(fill(state.thumb_fill_bounds, state.thumb_bg).corner_radii(radius));
            });

            cx.on_mouse_event({
                let state = self.state.clone();
                let view_id = self.view_id;
                let scroll_handle = self.scroll_handle.clone();

                move |event: &ScrollWheelEvent, phase, cx| {
                    if phase.bubble()
                        && hitbox_bounds.contains(&event.position)
                        && scroll_handle.offset() != state.get().last_scroll_offset
                    {
                        state.set(
                            state
                                .get()
                                .with_last_scroll(scroll_handle.offset(), Some(Instant::now())),
                        );
                        cx.notify(Some(view_id));
                    }
                }
            });

            let safe_range = (-scroll_area_size + container_size)..px(0.);

            if is_hover_to_show || is_visible {
                cx.on_mouse_event({
                    let state = self.state.clone();
                    let view_id = self.view_id;
                    let scroll_handle = self.scroll_handle.clone();

                    move |event: &MouseDownEvent, phase, cx| {
                        if phase.bubble() && bounds.contains(&event.position) {
                            cx.stop_propagation();

                            if thumb_bounds.contains(&event.position) {
                                // click on the thumb bar, set the drag position
                                let pos = event.position - thumb_bounds.origin;

                                state.set(state.get().with_drag_pos(axis, pos));

                                cx.notify(Some(view_id));
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

            cx.on_mouse_event({
                let scroll_handle = self.scroll_handle.clone();
                let state = self.state.clone();
                let view_id = self.view_id;

                move |event: &MouseMoveEvent, _, cx| {
                    // Update hovered state for scrollbar
                    if bounds.contains(&event.position) {
                        if state.get().hovered_axis != Some(axis) {
                            state.set(state.get().with_hovered(Some(axis)));
                            cx.notify(Some(view_id));
                        }
                    } else if state.get().hovered_axis == Some(axis)
                        && state.get().hovered_axis.is_some()
                    {
                        state.set(state.get().with_hovered(None));
                        cx.notify(Some(view_id));
                    }

                    // Update hovered state for scrollbar thumb
                    if thumb_bounds.contains(&event.position) {
                        if state.get().hovered_on_thumb != Some(axis) {
                            state.set(state.get().with_hovered_on_thumb(Some(axis)));
                            cx.notify(Some(view_id));
                        }
                    } else if state.get().hovered_on_thumb == Some(axis) {
                        state.set(state.get().with_hovered_on_thumb(None));
                        cx.notify(Some(view_id));
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

                        scroll_handle.set_offset(offset);
                        cx.notify(Some(view_id));
                    }
                }
            });

            cx.on_mouse_event({
                let view_id = self.view_id;
                let state = self.state.clone();

                move |_event: &MouseUpEvent, phase, cx| {
                    if phase.bubble() {
                        state.set(state.get().with_unset_drag_pos());
                        cx.notify(Some(view_id));
                    }
                }
            });
        }
    }
}