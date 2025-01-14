use super::{
    panel::PanelView, stack_panel::StackPanel, ClosePanel, DockArea, PanelEvent, PanelStyle,
    ToggleZoom,
};
use crate::{
    button::{Button, ButtonVariants as _},
    dock_area::{
        dock::DockPlacement,
        panel::Panel,
        state::{PanelInfo, PanelState},
    },
    h_flex,
    popup_menu::{PopupMenu, PopupMenuExt},
    tab::{tab_bar::TabBar, Tab},
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, AxisExt, IconName, Placement, Selectable, Sizable,
};
use gpui::{
    div, img, prelude::FluentBuilder, px, rems, AppContext, Corner, DefiniteLength, DismissEvent,
    DragMoveEvent, Empty, Entity, EventEmitter, FocusHandle, FocusableView,
    InteractiveElement as _, IntoElement, ObjectFit, ParentElement, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, View, ViewContext,
    VisualContext as _, WeakView, WindowContext,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
struct TabState {
    closeable: bool,
    zoomable: bool,
    draggable: bool,
    droppable: bool,
}

#[derive(Clone)]
pub(crate) struct DragPanel {
    pub(crate) panel: Arc<dyn PanelView>,
    pub(crate) tab_panel: View<TabPanel>,
}

impl DragPanel {
    pub(crate) fn new(panel: Arc<dyn PanelView>, tab_panel: View<TabPanel>) -> Self {
        Self { panel, tab_panel }
    }
}

impl Render for DragPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .id("drag-panel")
            .cursor_grab()
            .py_1()
            .px_2()
            .w_24()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .rounded(px(cx.theme().radius))
            .text_xs()
            .border_1()
            .border_color(cx.theme().base.step(cx, ColorScaleStep::THREE))
            .bg(cx.theme().base.step(cx, ColorScaleStep::TWO))
            .child(self.panel.title(cx))
    }
}

pub struct TabPanel {
    focus_handle: FocusHandle,
    dock_area: WeakView<DockArea>,
    /// The stock_panel can be None, if is None, that means the panels can't be split or move
    stack_panel: Option<WeakView<StackPanel>>,
    pub(crate) panels: Vec<Arc<dyn PanelView>>,
    pub(crate) active_ix: usize,
    /// If this is true, the Panel closeable will follow the active panel's closeable,
    /// otherwise this TabPanel will not able to close
    pub(crate) closeable: bool,
    tab_bar_scroll_handle: ScrollHandle,
    is_zoomed: bool,
    is_collapsed: bool,
    /// When drag move, will get the placement of the panel to be split
    will_split_placement: Option<Placement>,
}

impl Panel for TabPanel {
    fn panel_id(&self) -> SharedString {
        "TabPanel".into()
    }

    fn title(&self, cx: &WindowContext) -> gpui::AnyElement {
        self.active_panel()
            .map(|panel| panel.title(cx))
            .unwrap_or("Empty Tab".into_any_element())
    }

    fn closeable(&self, cx: &WindowContext) -> bool {
        if !self.closeable {
            return false;
        }

        self.active_panel()
            .map(|panel| panel.closeable(cx))
            .unwrap_or(false)
    }

    fn zoomable(&self, cx: &WindowContext) -> bool {
        self.active_panel()
            .map(|panel| panel.zoomable(cx))
            .unwrap_or(false)
    }

    fn popup_menu(&self, menu: PopupMenu, cx: &WindowContext) -> PopupMenu {
        if let Some(panel) = self.active_panel() {
            panel.popup_menu(menu, cx)
        } else {
            menu
        }
    }

    fn toolbar_buttons(&self, cx: &WindowContext) -> Vec<Button> {
        if let Some(panel) = self.active_panel() {
            panel.toolbar_buttons(cx)
        } else {
            vec![]
        }
    }

    fn dump(&self, cx: &AppContext) -> PanelState {
        let mut state = PanelState::new(self);
        for panel in self.panels.iter() {
            state.add_child(panel.dump(cx));
            state.info = PanelInfo::tabs(self.active_ix);
        }
        state
    }
}

impl TabPanel {
    pub fn new(
        stack_panel: Option<WeakView<StackPanel>>,
        dock_area: WeakView<DockArea>,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            dock_area,
            stack_panel,
            panels: Vec::new(),
            active_ix: 0,
            tab_bar_scroll_handle: ScrollHandle::new(),
            will_split_placement: None,
            is_zoomed: false,
            is_collapsed: false,
            closeable: true,
        }
    }

    pub(super) fn set_parent(&mut self, view: WeakView<StackPanel>) {
        self.stack_panel = Some(view);
    }

    /// Return current active_panel View
    pub fn active_panel(&self) -> Option<Arc<dyn PanelView>> {
        self.panels.get(self.active_ix).cloned()
    }

    fn set_active_ix(&mut self, ix: usize, cx: &mut ViewContext<Self>) {
        self.active_ix = ix;
        self.tab_bar_scroll_handle.scroll_to_item(ix);
        self.focus_active_panel(cx);
        cx.emit(PanelEvent::LayoutChanged);
        cx.notify();
    }

    /// Add a panel to the end of the tabs
    pub fn add_panel(&mut self, panel: Arc<dyn PanelView>, cx: &mut ViewContext<Self>) {
        self.add_panel_with_active(panel, true, cx);
    }

    fn add_panel_with_active(
        &mut self,
        panel: Arc<dyn PanelView>,
        active: bool,
        cx: &mut ViewContext<Self>,
    ) {
        if self
            .panels
            .iter()
            .any(|p| p.panel_id(cx) == panel.panel_id(cx))
        {
            // Set the active panel to the matched panel
            if active {
                if let Some(ix) = self
                    .panels
                    .iter()
                    .position(|p| p.panel_id(cx) == panel.panel_id(cx))
                {
                    self.set_active_ix(ix, cx);
                }
            }

            return;
        }

        self.panels.push(panel);

        // Set the active panel to the new panel
        if active {
            self.set_active_ix(self.panels.len() - 1, cx);
        }

        cx.emit(PanelEvent::LayoutChanged);
        cx.notify();
    }

    /// Add panel to try to split
    pub fn add_panel_at(
        &mut self,
        panel: Arc<dyn PanelView>,
        placement: Placement,
        size: Option<Pixels>,
        cx: &mut ViewContext<Self>,
    ) {
        cx.spawn(|view, mut cx| async move {
            cx.update(|cx| {
                view.update(cx, |view, cx| {
                    view.will_split_placement = Some(placement);
                    view.split_panel(panel, placement, size, cx)
                })
                .ok()
            })
            .ok()
        })
        .detach();
        cx.emit(PanelEvent::LayoutChanged);
        cx.notify();
    }

    fn insert_panel_at(
        &mut self,
        panel: Arc<dyn PanelView>,
        ix: usize,
        cx: &mut ViewContext<Self>,
    ) {
        if self
            .panels
            .iter()
            .any(|p| p.view().entity_id() == panel.view().entity_id())
        {
            return;
        }

        self.panels.insert(ix, panel);
        self.set_active_ix(ix, cx);
        cx.emit(PanelEvent::LayoutChanged);
        cx.notify();
    }

    /// Remove a panel from the tab panel
    pub fn remove_panel(&mut self, panel: Arc<dyn PanelView>, cx: &mut ViewContext<Self>) {
        self.detach_panel(panel, cx);
        self.remove_self_if_empty(cx);
        cx.emit(PanelEvent::ZoomOut);
        cx.emit(PanelEvent::LayoutChanged);
    }

    fn detach_panel(&mut self, panel: Arc<dyn PanelView>, cx: &mut ViewContext<Self>) {
        let panel_view = panel.view();
        self.panels.retain(|p| p.view() != panel_view);
        if self.active_ix >= self.panels.len() {
            self.set_active_ix(self.panels.len().saturating_sub(1), cx)
        }
    }

    /// Check to remove self from the parent StackPanel, if there is no panel left
    fn remove_self_if_empty(&self, cx: &mut ViewContext<Self>) {
        if !self.panels.is_empty() {
            return;
        }

        let tab_view = cx.view().clone();
        if let Some(stack_panel) = self.stack_panel.as_ref() {
            _ = stack_panel.update(cx, |view, cx| {
                view.remove_panel(Arc::new(tab_view), cx);
            });
        }
    }

    pub(super) fn set_collapsed(&mut self, collapsed: bool, cx: &mut ViewContext<Self>) {
        self.is_collapsed = collapsed;
        cx.notify();
    }

    fn is_locked(&self, cx: &AppContext) -> bool {
        let Some(dock_area) = self.dock_area.upgrade() else {
            return true;
        };

        if dock_area.read(cx).is_locked() {
            return true;
        }

        if self.is_zoomed {
            return true;
        }

        self.stack_panel.is_none()
    }

    /// Return true if self or parent only have last panel.
    fn is_last_panel(&self, cx: &AppContext) -> bool {
        if let Some(parent) = &self.stack_panel {
            if let Some(stack_panel) = parent.upgrade() {
                if !stack_panel.read(cx).is_last_panel(cx) {
                    return false;
                }
            }
        }

        self.panels.len() <= 1
    }

    /// Return true if the tab panel is draggable.
    ///
    /// E.g. if the parent and self only have one panel, it is not draggable.
    fn draggable(&self, cx: &AppContext) -> bool {
        !self.is_locked(cx) && !self.is_last_panel(cx)
    }

    /// Return true if the tab panel is droppable.
    ///
    /// E.g. if the tab panel is locked, it is not droppable.
    fn droppable(&self, cx: &AppContext) -> bool {
        !self.is_locked(cx)
    }

    fn render_toolbar(&self, state: TabState, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let is_zoomed = self.is_zoomed && state.zoomable;
        let view = cx.view().clone();
        let build_popup_menu = move |this, cx: &WindowContext| view.read(cx).popup_menu(this, cx);

        // TODO: Do not show MenuButton if there is no menu items

        h_flex()
            .gap_2()
            .occlude()
            .items_center()
            .children(
                self.toolbar_buttons(cx)
                    .into_iter()
                    .map(|btn| btn.xsmall().ghost()),
            )
            .when(self.is_zoomed, |this| {
                this.child(
                    Button::new("zoom")
                        .icon(IconName::Minimize)
                        .xsmall()
                        .ghost()
                        .tooltip("Zoom Out")
                        .on_click(
                            cx.listener(|view, _, cx| view.on_action_toggle_zoom(&ToggleZoom, cx)),
                        ),
                )
            })
            .child(
                Button::new("menu")
                    .icon(IconName::Ellipsis)
                    .xsmall()
                    .ghost()
                    .popup_menu(move |this, cx| {
                        build_popup_menu(this, cx)
                            .when(state.zoomable, |this| {
                                let name = if is_zoomed { "Zoom Out" } else { "Zoom In" };
                                this.separator().menu(name, Box::new(ToggleZoom))
                            })
                            .when(state.closeable, |this| {
                                this.separator().menu("Close", Box::new(ClosePanel))
                            })
                    })
                    .anchor(Corner::TopRight),
            )
    }

    fn render_dock_toggle_button(
        &self,
        placement: DockPlacement,
        cx: &mut ViewContext<Self>,
    ) -> Option<impl IntoElement> {
        if self.is_zoomed {
            return None;
        }

        let dock_area = self.dock_area.upgrade()?.read(cx);
        if !dock_area.is_dock_collapsible(placement, cx) {
            return None;
        }

        let view_entity_id = cx.view().entity_id();
        let toggle_button_panels = dock_area.toggle_button_panels;

        // Check if current TabPanel's entity_id matches the one stored in DockArea for this placement
        if !match placement {
            DockPlacement::Left => {
                dock_area.left_dock.is_some() && toggle_button_panels.left == Some(view_entity_id)
            }
            DockPlacement::Right => {
                dock_area.right_dock.is_some() && toggle_button_panels.right == Some(view_entity_id)
            }
            DockPlacement::Bottom => {
                dock_area.bottom_dock.is_some()
                    && toggle_button_panels.bottom == Some(view_entity_id)
            }
            DockPlacement::Center => unreachable!(),
        } {
            return None;
        }

        let is_open = dock_area.is_dock_open(placement, cx);

        let icon = match placement {
            DockPlacement::Left => {
                if is_open {
                    IconName::PanelLeft
                } else {
                    IconName::PanelLeftOpen
                }
            }
            DockPlacement::Right => {
                if is_open {
                    IconName::PanelRight
                } else {
                    IconName::PanelRightOpen
                }
            }
            DockPlacement::Bottom => {
                if is_open {
                    IconName::PanelBottom
                } else {
                    IconName::PanelBottomOpen
                }
            }
            DockPlacement::Center => unreachable!(),
        };

        Some(
            Button::new(SharedString::from(format!("toggle-dock:{:?}", placement)))
                .icon(icon)
                .xsmall()
                .ghost()
                .tooltip(match is_open {
                    true => "Collapse",
                    false => "Expand",
                })
                .on_click(cx.listener({
                    let dock_area = self.dock_area.clone();
                    move |_, _, cx| {
                        _ = dock_area.update(cx, |dock_area, cx| {
                            dock_area.toggle_dock(placement, cx);
                        });
                    }
                })),
        )
    }

    fn render_title_bar(&self, state: TabState, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let view = cx.view().clone();

        let Some(dock_area) = self.dock_area.upgrade() else {
            return div().into_any_element();
        };

        let panel_style = dock_area.read(cx).panel_style;

        let left_dock_button = self.render_dock_toggle_button(DockPlacement::Left, cx);
        let bottom_dock_button = self.render_dock_toggle_button(DockPlacement::Bottom, cx);
        let right_dock_button = self.render_dock_toggle_button(DockPlacement::Right, cx);

        if self.panels.len() == 1 && panel_style == PanelStyle::Default {
            let panel = self.panels.first().unwrap();

            return h_flex()
                .justify_between()
                .items_center()
                .line_height(rems(1.0))
                .h(px(30.))
                .py_2()
                .px_3()
                .when(left_dock_button.is_some(), |this| this.pl_2())
                .when(right_dock_button.is_some(), |this| this.pr_2())
                .when(
                    left_dock_button.is_some() || bottom_dock_button.is_some(),
                    |this| {
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .mr_1()
                                .gap_1()
                                .children(left_dock_button)
                                .children(bottom_dock_button),
                        )
                    },
                )
                .child(
                    div()
                        .id("tab")
                        .flex_1()
                        .min_w_16()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_ellipsis()
                                .text_xs()
                                .child(div().when_some(
                                    panel.panel_metadata(cx),
                                    |this, metadata| {
                                        if let Some(picture) = metadata.picture {
                                            this.flex_shrink_0().child(
                                                img(format!(
                                                    "https://wsrv.nl/?url={}&w=100&h=100&n=-1",
                                                    picture
                                                ))
                                                .size_4()
                                                .rounded_full()
                                                .object_fit(ObjectFit::Cover),
                                            )
                                        } else {
                                            this.flex_shrink_0().child(
                                                img("brand/avatar.png").size_4().rounded_full(),
                                            )
                                        }
                                    },
                                ))
                                .child(panel.title(cx)),
                        )
                        .when(state.draggable, |this| {
                            this.on_drag(
                                DragPanel {
                                    panel: panel.clone(),
                                    tab_panel: view,
                                },
                                |drag, _, cx| {
                                    cx.stop_propagation();
                                    cx.new_view(|_| drag.clone())
                                },
                            )
                        }),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .ml_1()
                        .gap_1()
                        .child(self.render_toolbar(state, cx))
                        .children(right_dock_button),
                )
                .into_any_element();
        }

        let tabs_count = self.panels.len();

        TabBar::new("tab-bar")
            .track_scroll(self.tab_bar_scroll_handle.clone())
            .when(
                left_dock_button.is_some() || bottom_dock_button.is_some(),
                |this| {
                    this.prefix(
                        h_flex()
                            .items_center()
                            .top_0()
                            // Right -1 for avoid border overlap with the first tab
                            .right(-px(1.))
                            .border_r_1()
                            .border_b_1()
                            .h_full()
                            .border_color(cx.theme().base.step(cx, ColorScaleStep::THREE))
                            .bg(cx.theme().base.step(cx, ColorScaleStep::TWO))
                            .px_2()
                            .children(left_dock_button)
                            .children(bottom_dock_button),
                    )
                },
            )
            .children(self.panels.iter().enumerate().map(|(ix, panel)| {
                let mut active = ix == self.active_ix;
                let disabled = self.is_collapsed;

                // Always not show active tab style, if the panel is collapsed
                if self.is_collapsed {
                    active = false;
                }

                Tab::new(("tab", ix), panel.title(cx), panel.panel_metadata(cx))
                    .py_2()
                    .selected(active)
                    .disabled(disabled)
                    .when(!disabled, |this| {
                        this.on_click(cx.listener(move |view, _, cx| {
                            view.set_active_ix(ix, cx);
                        }))
                        .when(state.draggable, |this| {
                            this.on_drag(
                                DragPanel::new(panel.clone(), view.clone()),
                                |drag, _, cx| {
                                    cx.stop_propagation();
                                    cx.new_view(|_| drag.clone())
                                },
                            )
                        })
                        .when(state.droppable, |this| {
                            this.drag_over::<DragPanel>(|this, _, cx| {
                                this.rounded_l_none()
                                    .border_l_2()
                                    .border_r_0()
                                    .border_color(cx.theme().base.step(cx, ColorScaleStep::THREE))
                            })
                            .on_drop(cx.listener(
                                move |this, drag: &DragPanel, cx| {
                                    this.will_split_placement = None;
                                    this.on_drop(drag, Some(ix), true, cx)
                                },
                            ))
                        })
                    })
            }))
            .child(
                // empty space to allow move to last tab right
                div()
                    .id("tab-bar-empty-space")
                    .h_full()
                    .flex_grow()
                    .min_w_16()
                    .when(state.droppable, |this| {
                        this.drag_over::<DragPanel>(|this, _, cx| {
                            this.bg(cx.theme().base.step(cx, ColorScaleStep::TWO))
                        })
                        .on_drop(cx.listener(
                            move |this, drag: &DragPanel, cx| {
                                this.will_split_placement = None;

                                let ix = if drag.tab_panel == view {
                                    Some(tabs_count - 1)
                                } else {
                                    None
                                };

                                this.on_drop(drag, ix, false, cx)
                            },
                        ))
                    }),
            )
            .suffix(
                h_flex()
                    .items_center()
                    .top_0()
                    .right_0()
                    .border_l_1()
                    .border_b_1()
                    .h_full()
                    .border_color(cx.theme().base.step(cx, ColorScaleStep::THREE))
                    .bg(cx.theme().base.step(cx, ColorScaleStep::TWO))
                    .px_2()
                    .gap_1()
                    .child(self.render_toolbar(state, cx))
                    .when_some(right_dock_button, |this, btn| this.child(btn)),
            )
            .into_any_element()
    }

    fn render_active_panel(&self, state: TabState, cx: &mut ViewContext<Self>) -> impl IntoElement {
        if self.is_collapsed {
            return Empty {}.into_any_element();
        }

        self.active_panel()
            .map(|panel| {
                div()
                    .id("tab-content")
                    .group("")
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .flex_1()
                    .child(panel.view())
                    .when(state.droppable, |this| {
                        this.on_drag_move(cx.listener(Self::on_panel_drag_move))
                            .child(
                                div()
                                    .invisible()
                                    .absolute()
                                    .bg(cx.theme().base.step(cx, ColorScaleStep::THREE))
                                    .map(|this| match self.will_split_placement {
                                        Some(placement) => {
                                            let size = DefiniteLength::Fraction(0.35);
                                            match placement {
                                                Placement::Left => {
                                                    this.left_0().top_0().bottom_0().w(size)
                                                }
                                                Placement::Right => {
                                                    this.right_0().top_0().bottom_0().w(size)
                                                }
                                                Placement::Top => {
                                                    this.top_0().left_0().right_0().h(size)
                                                }
                                                Placement::Bottom => {
                                                    this.bottom_0().left_0().right_0().h(size)
                                                }
                                            }
                                        }
                                        None => this.top_0().left_0().size_full(),
                                    })
                                    .group_drag_over::<DragPanel>("", |this| this.visible())
                                    .on_drop(cx.listener(|this, drag: &DragPanel, cx| {
                                        this.on_drop(drag, None, true, cx)
                                    })),
                            )
                    })
                    .into_any_element()
            })
            .unwrap_or(Empty {}.into_any_element())
    }

    /// Calculate the split direction based on the current mouse position
    fn on_panel_drag_move(&mut self, drag: &DragMoveEvent<DragPanel>, cx: &mut ViewContext<Self>) {
        let bounds = drag.bounds;
        let position = drag.event.position;

        // Check the mouse position to determine the split direction
        if position.x < bounds.left() + bounds.size.width * 0.35 {
            self.will_split_placement = Some(Placement::Left);
        } else if position.x > bounds.left() + bounds.size.width * 0.65 {
            self.will_split_placement = Some(Placement::Right);
        } else if position.y < bounds.top() + bounds.size.height * 0.35 {
            self.will_split_placement = Some(Placement::Top);
        } else if position.y > bounds.top() + bounds.size.height * 0.65 {
            self.will_split_placement = Some(Placement::Bottom);
        } else {
            // center to merge into the current tab
            self.will_split_placement = None;
        }
        cx.notify()
    }

    /// Handle the drop event when dragging a panel
    ///
    /// - `active` - When true, the panel will be active after the drop
    fn on_drop(
        &mut self,
        drag: &DragPanel,
        ix: Option<usize>,
        active: bool,
        cx: &mut ViewContext<Self>,
    ) {
        let panel = drag.panel.clone();
        let is_same_tab = drag.tab_panel == *cx.view();

        // If target is same tab, and it is only one panel, do nothing.
        if is_same_tab && ix.is_none() {
            #[allow(clippy::if_same_then_else)]
            if self.will_split_placement.is_none() {
                return;
            } else if self.panels.len() == 1 {
                return;
            }
        }

        // Here is looks like remove_panel on a same item, but it difference.
        //
        // We must to split it to remove_panel, unless it will be crash by error:
        // Cannot update ui::dock::tab_panel::TabPanel while it is already being updated
        if is_same_tab {
            self.detach_panel(panel.clone(), cx);
        } else {
            drag.tab_panel.update(cx, |view, cx| {
                view.detach_panel(panel.clone(), cx);
                view.remove_self_if_empty(cx);
            });
        }

        // Insert into new tabs
        if let Some(placement) = self.will_split_placement {
            self.split_panel(panel, placement, None, cx);
        } else if let Some(ix) = ix {
            self.insert_panel_at(panel, ix, cx)
        } else {
            self.add_panel_with_active(panel, active, cx)
        }

        self.remove_self_if_empty(cx);
        cx.emit(PanelEvent::LayoutChanged);
    }

    /// Add panel with split placement
    fn split_panel(
        &self,
        panel: Arc<dyn PanelView>,
        placement: Placement,
        size: Option<Pixels>,
        cx: &mut ViewContext<Self>,
    ) {
        let dock_area = self.dock_area.clone();
        // wrap the panel in a TabPanel
        let new_tab_panel = cx.new_view(|cx| Self::new(None, dock_area.clone(), cx));
        new_tab_panel.update(cx, |view, cx| {
            view.add_panel(panel, cx);
        });

        let stack_panel = match self.stack_panel.as_ref().and_then(|panel| panel.upgrade()) {
            Some(panel) => panel,
            None => return,
        };

        let parent_axis = stack_panel.read(cx).axis;

        let ix = stack_panel
            .read(cx)
            .index_of_panel(Arc::new(cx.view().clone()))
            .unwrap_or_default();

        if parent_axis.is_vertical() && placement.is_vertical() {
            stack_panel.update(cx, |view, cx| {
                view.insert_panel_at(
                    Arc::new(new_tab_panel),
                    ix,
                    placement,
                    size,
                    dock_area.clone(),
                    cx,
                );
            });
        } else if parent_axis.is_horizontal() && placement.is_horizontal() {
            stack_panel.update(cx, |view, cx| {
                view.insert_panel_at(
                    Arc::new(new_tab_panel),
                    ix,
                    placement,
                    size,
                    dock_area.clone(),
                    cx,
                );
            });
        } else {
            // 1. Create new StackPanel with new axis
            // 2. Move cx.view() from parent StackPanel to the new StackPanel
            // 3. Add the new TabPanel to the new StackPanel at the correct index
            // 4. Add new StackPanel to the parent StackPanel at the correct index
            let tab_panel = cx.view().clone();

            // Try to use the old stack panel, not just create a new one, to avoid too many nested stack panels
            let new_stack_panel = if stack_panel.read(cx).panels_len() <= 1 {
                stack_panel.update(cx, |view, cx| {
                    view.remove_all_panels(cx);
                    view.set_axis(placement.axis(), cx);
                });
                stack_panel.clone()
            } else {
                cx.new_view(|cx| {
                    let mut panel = StackPanel::new(placement.axis(), cx);
                    panel.parent = Some(stack_panel.downgrade());
                    panel
                })
            };

            new_stack_panel.update(cx, |view, cx| match placement {
                Placement::Left | Placement::Top => {
                    view.add_panel(Arc::new(new_tab_panel), size, dock_area.clone(), cx);
                    view.add_panel(Arc::new(tab_panel.clone()), None, dock_area.clone(), cx);
                }
                Placement::Right | Placement::Bottom => {
                    view.add_panel(Arc::new(tab_panel.clone()), None, dock_area.clone(), cx);
                    view.add_panel(Arc::new(new_tab_panel), size, dock_area.clone(), cx);
                }
            });

            if stack_panel != new_stack_panel {
                stack_panel.update(cx, |view, cx| {
                    view.replace_panel(Arc::new(tab_panel.clone()), new_stack_panel.clone(), cx);
                });
            }

            cx.spawn(|_, mut cx| async move {
                cx.update(|cx| tab_panel.update(cx, |view, cx| view.remove_self_if_empty(cx)))
            })
            .detach()
        }

        cx.emit(PanelEvent::LayoutChanged);
    }

    fn focus_active_panel(&self, cx: &mut ViewContext<Self>) {
        if let Some(active_panel) = self.active_panel() {
            active_panel.focus_handle(cx).focus(cx);
        }
    }

    fn on_action_toggle_zoom(&mut self, _: &ToggleZoom, cx: &mut ViewContext<Self>) {
        if !self.zoomable(cx) {
            return;
        }

        if !self.is_zoomed {
            cx.emit(PanelEvent::ZoomIn)
        } else {
            cx.emit(PanelEvent::ZoomOut)
        }
        self.is_zoomed = !self.is_zoomed;
    }

    fn on_action_close_panel(&mut self, _: &ClosePanel, cx: &mut ViewContext<Self>) {
        if let Some(panel) = self.active_panel() {
            self.remove_panel(panel, cx);
        }
    }
}

impl FocusableView for TabPanel {
    fn focus_handle(&self, cx: &AppContext) -> gpui::FocusHandle {
        if let Some(active_panel) = self.active_panel() {
            active_panel.focus_handle(cx)
        } else {
            self.focus_handle.clone()
        }
    }
}

impl EventEmitter<DismissEvent> for TabPanel {}

impl EventEmitter<PanelEvent> for TabPanel {}

impl Render for TabPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl gpui::IntoElement {
        let focus_handle = self.focus_handle(cx);
        let mut state = TabState {
            closeable: self.closeable(cx),
            draggable: self.draggable(cx),
            droppable: self.droppable(cx),
            zoomable: self.zoomable(cx),
        };
        if !state.draggable {
            state.closeable = false;
        }

        v_flex()
            .id("tab-panel")
            .track_focus(&focus_handle)
            .on_action(cx.listener(Self::on_action_toggle_zoom))
            .on_action(cx.listener(Self::on_action_close_panel))
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(self.render_title_bar(state, cx))
            .child(self.render_active_panel(state, cx))
    }
}
