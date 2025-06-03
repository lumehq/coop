use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext, Axis, Context, Element, Entity, InteractiveElement as _, IntoElement,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Render,
    StatefulInteractiveElement, Style, Styled as _, WeakEntity, Window,
};
use serde::{Deserialize, Serialize};
use theme::ActiveTheme;

use super::{DockArea, DockItem};
use crate::dock_area::panel::PanelView;
use crate::dock_area::tab_panel::TabPanel;
use crate::resizable::{HANDLE_PADDING, HANDLE_SIZE, PANEL_MIN_SIZE};
use crate::{AxisExt as _, StyledExt};

#[derive(Clone, Render)]
struct ResizePanel;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DockPlacement {
    #[serde(rename = "center")]
    Center,
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "bottom")]
    Bottom,
    #[serde(rename = "right")]
    Right,
}

impl DockPlacement {
    fn axis(&self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Bottom => Axis::Vertical,
            Self::Center => unreachable!(),
        }
    }

    pub fn is_left(&self) -> bool {
        matches!(self, Self::Left)
    }

    pub fn is_bottom(&self) -> bool {
        matches!(self, Self::Bottom)
    }

    pub fn is_right(&self) -> bool {
        matches!(self, Self::Right)
    }
}

/// The Dock is a fixed container that places at left, bottom, right of the Windows.
///
/// This is unlike Panel, it can't be move or add any other panel.
pub struct Dock {
    pub(super) placement: DockPlacement,
    dock_area: WeakEntity<DockArea>,
    pub(crate) panel: DockItem,
    /// The size is means the width or height of the Dock, if the placement is left or right, the size is width, otherwise the size is height.
    pub(super) size: Pixels,
    pub(super) open: bool,
    /// Whether the Dock is collapsible, default: true
    pub(super) collapsible: bool,

    // Runtime state
    /// Whether the Dock is resizing
    is_resizing: bool,
}

impl Dock {
    pub(crate) fn new(
        dock_area: WeakEntity<DockArea>,
        placement: DockPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let panel = cx.new(|cx| {
            let mut tab = TabPanel::new(None, dock_area.clone(), window, cx);
            tab.closable = true;
            tab
        });

        let panel = DockItem::Tabs {
            items: Vec::new(),
            active_ix: 0,
            view: panel.clone(),
        };

        Self::subscribe_panel_events(dock_area.clone(), &panel, window, cx);

        Self {
            placement,
            dock_area,
            panel,
            open: true,
            collapsible: true,
            size: px(200.0),
            is_resizing: false,
        }
    }

    pub fn left(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(dock_area, DockPlacement::Left, window, cx)
    }

    pub fn bottom(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(dock_area, DockPlacement::Bottom, window, cx)
    }

    pub fn right(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(dock_area, DockPlacement::Right, window, cx)
    }

    /// Update the Dock to be collapsible or not.
    ///
    /// And if the Dock is not collapsible, it will be open.
    pub fn set_collapsible(
        &mut self,
        collapsible: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collapsible = collapsible;
        if !collapsible {
            self.open = true
        }
        cx.notify();
    }

    fn subscribe_panel_events(
        dock_area: WeakEntity<DockArea>,
        panel: &DockItem,
        window: &mut Window,
        cx: &mut App,
    ) {
        match panel {
            DockItem::Tabs { view, .. } => {
                window.defer(cx, {
                    let view = view.clone();
                    move |window, cx| {
                        _ = dock_area.update(cx, |this, cx| {
                            this.subscribe_panel(&view, window, cx);
                        });
                    }
                });
            }
            DockItem::Split { items, view, .. } => {
                for item in items {
                    Self::subscribe_panel_events(dock_area.clone(), item, window, cx);
                }
                window.defer(cx, {
                    let view = view.clone();
                    move |window, cx| {
                        _ = dock_area.update(cx, |this, cx| {
                            this.subscribe_panel(&view, window, cx);
                        });
                    }
                });
            }
            DockItem::Panel { .. } => {
                // Not supported
            }
        }
    }

    pub fn set_panel(&mut self, panel: DockItem, _window: &mut Window, cx: &mut Context<Self>) {
        self.panel = panel;
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(!self.open, window, cx);
    }

    /// Returns the size of the Dock, the size is means the width or height of
    /// the Dock, if the placement is left or right, the size is width,
    /// otherwise the size is height.
    pub fn size(&self) -> Pixels {
        self.size
    }

    /// Set the size of the Dock.
    pub fn set_size(&mut self, size: Pixels, _window: &mut Window, cx: &mut Context<Self>) {
        self.size = size.max(PANEL_MIN_SIZE);
        cx.notify();
    }

    /// Set the open state of the Dock.
    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open = open;
        let item = self.panel.clone();
        cx.defer_in(window, move |_, window, cx| {
            item.set_collapsed(!open, window, cx);
        });
        cx.notify();
    }

    /// Add item to the Dock.
    pub fn add_panel(
        &mut self,
        panel: Arc<dyn PanelView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.panel.add_panel(panel, &self.dock_area, window, cx);
        cx.notify();
    }

    fn render_resize_handle(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let axis = self.placement.axis();
        let neg_offset = -HANDLE_PADDING;
        let view = cx.entity().clone();

        div()
            .id("resize-handle")
            .occlude()
            .absolute()
            .flex_shrink_0()
            .when(self.placement.is_left(), |this| {
                // FIXME: Improve this to let the scroll bar have px(HANDLE_PADDING)
                this.cursor_col_resize()
                    .top_0()
                    .right(px(1.))
                    .h_full()
                    .w(HANDLE_SIZE)
                    .pt_12()
                    .pb_4()
            })
            .when(self.placement.is_right(), |this| {
                this.cursor_col_resize()
                    .top_0()
                    .left(px(-0.5))
                    .h_full()
                    .w(HANDLE_SIZE)
                    .pt_12()
                    .pb_4()
            })
            .when(self.placement.is_bottom(), |this| {
                this.cursor_row_resize()
                    .top(neg_offset)
                    .left_0()
                    .w_full()
                    .h(HANDLE_SIZE)
                    .py(HANDLE_PADDING)
            })
            .child(
                div()
                    .rounded_full()
                    .hover(|this| this.bg(cx.theme().border_variant))
                    .when(axis.is_horizontal(), |this| this.h_full().w(HANDLE_SIZE))
                    .when(axis.is_vertical(), |this| this.w_full().h(HANDLE_SIZE)),
            )
            .on_drag(ResizePanel {}, move |info, _, _, cx| {
                cx.stop_propagation();
                view.update(cx, |view, _| {
                    view.is_resizing = true;
                });
                cx.new(|_| info.clone())
            })
    }

    fn resize(
        &mut self,
        mouse_position: Point<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_resizing {
            return;
        }

        let dock_area = self
            .dock_area
            .upgrade()
            .expect("DockArea is missing")
            .read(cx);

        let area_bounds = dock_area.bounds;
        let mut left_dock_size = Pixels(0.0);
        let mut right_dock_size = Pixels(0.0);

        // Get the size of the left dock if it's open and not the current dock
        if let Some(left_dock) = &dock_area.left_dock {
            if left_dock.entity_id() != cx.entity().entity_id() {
                let left_dock_read = left_dock.read(cx);
                if left_dock_read.is_open() {
                    left_dock_size = left_dock_read.size;
                }
            }
        }

        // Get the size of the right dock if it's open and not the current dock
        if let Some(right_dock) = &dock_area.right_dock {
            if right_dock.entity_id() != cx.entity().entity_id() {
                let right_dock_read = right_dock.read(cx);
                if right_dock_read.is_open() {
                    right_dock_size = right_dock_read.size;
                }
            }
        }

        let size = match self.placement {
            DockPlacement::Left => mouse_position.x - area_bounds.left(),
            DockPlacement::Right => area_bounds.right() - mouse_position.x,
            DockPlacement::Bottom => area_bounds.bottom() - mouse_position.y,
            DockPlacement::Center => unreachable!(),
        };

        match self.placement {
            DockPlacement::Left => {
                let max_size = area_bounds.size.width - PANEL_MIN_SIZE - right_dock_size;
                self.size = size.clamp(PANEL_MIN_SIZE, max_size);
            }
            DockPlacement::Right => {
                let max_size = area_bounds.size.width - PANEL_MIN_SIZE - left_dock_size;
                self.size = size.clamp(PANEL_MIN_SIZE, max_size);
            }
            DockPlacement::Bottom => {
                let max_size = area_bounds.size.height - PANEL_MIN_SIZE;
                self.size = size.clamp(PANEL_MIN_SIZE, max_size);
            }
            DockPlacement::Center => unreachable!(),
        }

        cx.notify();
    }

    fn done_resizing(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_resizing = false;
    }
}

impl Render for Dock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if !self.open && !self.placement.is_bottom() {
            return div();
        }

        div()
            .relative()
            .overflow_hidden()
            .map(|this| match self.placement {
                DockPlacement::Left | DockPlacement::Right => this.h_flex().h_full().w(self.size),
                DockPlacement::Bottom => this.w_full().h(self.size),
                DockPlacement::Center => unreachable!(),
            })
            // Bottom Dock should keep the title bar, then user can click the Toggle button
            .when(!self.open && self.placement.is_bottom(), |this| {
                this.h(px(29.))
            })
            .map(|this| match &self.panel {
                DockItem::Split { view, .. } => this.child(view.clone()),
                DockItem::Tabs { view, .. } => this.child(view.clone()),
                DockItem::Panel { view, .. } => this.child(view.clone().view()),
            })
            .child(self.render_resize_handle(window, cx))
            .child(DockElement {
                view: cx.entity().clone(),
            })
    }
}

struct DockElement {
    view: Entity<Dock>,
}

impl IntoElement for DockElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DockElement {
    type PrepaintState = ();
    type RequestLayoutState = ();

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
        window: &mut gpui::Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        (window.request_layout(Style::default(), None, cx), ())
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
        _: gpui::Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut gpui::Window,
        cx: &mut App,
    ) {
        window.on_mouse_event({
            let view = self.view.clone();
            let is_resizing = view.read(cx).is_resizing;
            move |e: &MouseMoveEvent, phase, window, cx| {
                if !is_resizing {
                    return;
                }
                if !phase.bubble() {
                    return;
                }

                view.update(cx, |view, cx| view.resize(e.position, window, cx))
            }
        });

        // When any mouse up, stop dragging
        window.on_mouse_event({
            let view = self.view.clone();
            move |_: &MouseUpEvent, phase, window, cx| {
                if phase.bubble() {
                    view.update(cx, |view, cx| view.done_resizing(window, cx));
                }
            }
        })
    }
}
