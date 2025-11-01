use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    actions, anchored, deferred, div, px, AnyElement, App, Bounds, Context, Corner, DismissEvent,
    DispatchPhase, Element, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    GlobalElementId, Hitbox, HitboxBehavior, InteractiveElement as _, IntoElement, KeyBinding,
    LayoutId, ManagedView, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render,
    ScrollHandle, StatefulInteractiveElement, Style, StyleRefinement, Styled, Window,
};

use crate::{Selectable, StyledExt as _};

const CONTEXT: &str = "Popover";

actions!(popover, [Escape]);

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Escape, Some(CONTEXT))])
}

type PopoverChild<T> = Rc<dyn Fn(&mut Window, &mut Context<T>) -> AnyElement>;

pub struct PopoverContent {
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    max_width: Option<Pixels>,
    max_height: Option<Pixels>,
    scrollable: bool,
    child: PopoverChild<Self>,
}

impl PopoverContent {
    pub fn new<B>(_window: &mut Window, cx: &mut App, content: B) -> Self
    where
        B: Fn(&mut Window, &mut Context<Self>) -> AnyElement + 'static,
    {
        let focus_handle = cx.focus_handle();
        let scroll_handle = ScrollHandle::default();

        Self {
            focus_handle,
            scroll_handle,
            child: Rc::new(content),
            max_width: None,
            max_height: None,
            scrollable: false,
        }
    }

    pub fn max_w(mut self, max_width: Pixels) -> Self {
        self.max_width = Some(max_width);
        self
    }

    pub fn max_h(mut self, max_height: Pixels) -> Self {
        self.max_height = Some(max_height);
        self
    }

    pub fn scrollable(mut self) -> Self {
        self.scrollable = true;
        self
    }
}

impl EventEmitter<DismissEvent> for PopoverContent {}

impl Focusable for PopoverContent {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PopoverContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("popup-content")
            .track_focus(&self.focus_handle)
            .key_context(CONTEXT)
            .on_action(cx.listener(|_, _: &Escape, _, cx| cx.emit(DismissEvent)))
            .p_2()
            .when(self.scrollable, |this| {
                this.overflow_y_scroll().track_scroll(&self.scroll_handle)
            })
            .when_some(self.max_width, |this, v| this.max_w(v))
            .when_some(self.max_height, |this, v| this.max_h(v))
            .child(self.child.clone()(window, cx))
    }
}

type Trigger = Option<Box<dyn FnOnce(bool, &Window, &App) -> AnyElement + 'static>>;
type Content<M> = Option<Rc<dyn Fn(&mut Window, &mut App) -> Entity<M> + 'static>>;

pub struct Popover<M: ManagedView> {
    id: ElementId,
    anchor: Corner,
    trigger: Trigger,
    content: Content<M>,
    /// Style for trigger element.
    /// This is used for hotfix the trigger element style to support w_full.
    trigger_style: Option<StyleRefinement>,
    mouse_button: MouseButton,
    no_style: bool,
}

impl<M> Popover<M>
where
    M: ManagedView,
{
    /// Create a new Popover with `view` mode.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            anchor: Corner::TopLeft,
            trigger: None,
            trigger_style: None,
            content: None,
            mouse_button: MouseButton::Left,
            no_style: false,
        }
    }

    pub fn anchor(mut self, anchor: Corner) -> Self {
        self.anchor = anchor;
        self
    }

    /// Set the mouse button to trigger the popover, default is `MouseButton::Left`.
    pub fn mouse_button(mut self, mouse_button: MouseButton) -> Self {
        self.mouse_button = mouse_button;
        self
    }

    pub fn trigger<T>(mut self, trigger: T) -> Self
    where
        T: Selectable + IntoElement + 'static,
    {
        self.trigger = Some(Box::new(|is_open, _, _| {
            trigger.selected(is_open).into_any_element()
        }));
        self
    }

    pub fn trigger_style(mut self, style: StyleRefinement) -> Self {
        self.trigger_style = Some(style);
        self
    }

    /// Set the content of the popover.
    ///
    /// The `content` is a closure that returns an `AnyElement`.
    pub fn content<C>(mut self, content: C) -> Self
    where
        C: Fn(&mut Window, &mut App) -> Entity<M> + 'static,
    {
        self.content = Some(Rc::new(content));
        self
    }

    /// Set whether the popover no style, default is `false`.
    ///
    /// If no style:
    ///
    /// - The popover will not have a bg, border, shadow, or padding.
    /// - The click out of the popover will not dismiss it.
    pub fn no_style(mut self) -> Self {
        self.no_style = true;
        self
    }

    fn render_trigger(&mut self, is_open: bool, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(trigger) = self.trigger.take() else {
            return div().into_any_element();
        };

        (trigger)(is_open, window, cx)
    }

    fn resolved_corner(&self, bounds: Bounds<Pixels>) -> Point<Pixels> {
        bounds.corner(match self.anchor {
            Corner::TopLeft => Corner::BottomLeft,
            Corner::TopRight => Corner::BottomRight,
            Corner::BottomLeft => Corner::TopLeft,
            Corner::BottomRight => Corner::TopRight,
        })
    }

    fn with_element_state<R>(
        &mut self,
        id: &GlobalElementId,
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(&mut Self, &mut PopoverElementState<M>, &mut Window, &mut App) -> R,
    ) -> R {
        window.with_optional_element_state::<PopoverElementState<M>, _>(
            Some(id),
            |element_state, window| {
                let mut element_state = element_state.unwrap().unwrap_or_default();
                let result = f(self, &mut element_state, window, cx);
                (result, Some(element_state))
            },
        )
    }
}

impl<M> IntoElement for Popover<M>
where
    M: ManagedView,
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct PopoverElementState<M> {
    trigger_layout_id: Option<LayoutId>,
    popover_layout_id: Option<LayoutId>,
    popover_element: Option<AnyElement>,
    trigger_element: Option<AnyElement>,
    content_view: Rc<RefCell<Option<Entity<M>>>>,
    /// Trigger bounds for positioning the popover.
    trigger_bounds: Option<Bounds<Pixels>>,
}

impl<M> Default for PopoverElementState<M> {
    fn default() -> Self {
        Self {
            trigger_layout_id: None,
            popover_layout_id: None,
            popover_element: None,
            trigger_element: None,
            content_view: Rc::new(RefCell::new(None)),
            trigger_bounds: None,
        }
    }
}

pub struct PrepaintState {
    hitbox: Hitbox,
    /// Trigger bounds for limit a rect to handle mouse click.
    trigger_bounds: Option<Bounds<Pixels>>,
}

impl<M: ManagedView> Element for Popover<M> {
    type PrepaintState = PrepaintState;
    type RequestLayoutState = PopoverElementState<M>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();

        // FIXME: Remove this and find a better way to handle this.
        // Apply trigger style, for support w_full for trigger.
        //
        // If remove this, the trigger will not support w_full.
        if let Some(trigger_style) = self.trigger_style.clone() {
            if let Some(width) = trigger_style.size.width {
                style.size.width = width;
            }
            if let Some(display) = trigger_style.display {
                style.display = display;
            }
        }

        self.with_element_state(
            id.unwrap(),
            window,
            cx,
            |view, element_state, window, cx| {
                let mut popover_layout_id = None;
                let mut popover_element = None;
                let mut is_open = false;

                if let Some(content_view) = element_state.content_view.borrow_mut().as_mut() {
                    is_open = true;

                    let mut anchored = anchored()
                        .snap_to_window_with_margin(px(8.))
                        .anchor(view.anchor);
                    if let Some(trigger_bounds) = element_state.trigger_bounds {
                        anchored = anchored.position(view.resolved_corner(trigger_bounds));
                    }

                    let mut element = {
                        let content_view_mut = element_state.content_view.clone();
                        let anchor = view.anchor;
                        let no_style = view.no_style;
                        deferred(
                            anchored.child(
                                div()
                                    .size_full()
                                    .occlude()
                                    .when(!no_style, |this| this.popover_style(cx))
                                    .map(|this| match anchor {
                                        Corner::TopLeft | Corner::TopRight => this.top_1p5(),
                                        Corner::BottomLeft | Corner::BottomRight => {
                                            this.bottom_1p5()
                                        }
                                    })
                                    .child(content_view.clone())
                                    .when(!no_style, |this| {
                                        this.on_mouse_down_out(move |_, window, _| {
                                            // Update the element_state.content_view to `None`,
                                            // so that the `paint`` method will not paint it.
                                            *content_view_mut.borrow_mut() = None;
                                            window.refresh();
                                        })
                                    }),
                            ),
                        )
                        .with_priority(1)
                        .into_any()
                    };

                    popover_layout_id = Some(element.request_layout(window, cx));
                    popover_element = Some(element);
                }

                let mut trigger_element = view.render_trigger(is_open, window, cx);
                let trigger_layout_id = trigger_element.request_layout(window, cx);

                let layout_id = window.request_layout(
                    style,
                    Some(trigger_layout_id).into_iter().chain(popover_layout_id),
                    cx,
                );

                (
                    layout_id,
                    PopoverElementState {
                        trigger_layout_id: Some(trigger_layout_id),
                        popover_layout_id,
                        popover_element,
                        trigger_element: Some(trigger_element),
                        ..Default::default()
                    },
                )
            },
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _bounds: gpui::Bounds<gpui::Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(element) = &mut request_layout.trigger_element {
            element.prepaint(window, cx);
        }
        if let Some(element) = &mut request_layout.popover_element {
            element.prepaint(window, cx);
        }

        let trigger_bounds = request_layout
            .trigger_layout_id
            .map(|id| window.layout_bounds(id));

        // Prepare the popover, for get the bounds of it for open window size.
        let _ = request_layout
            .popover_layout_id
            .map(|id| window.layout_bounds(id));

        let hitbox =
            window.insert_hitbox(trigger_bounds.unwrap_or_default(), HitboxBehavior::Normal);

        PrepaintState {
            trigger_bounds,
            hitbox,
        }
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.with_element_state(
            id.unwrap(),
            window,
            cx,
            |this, element_state, window, cx| {
                element_state.trigger_bounds = prepaint.trigger_bounds;

                if let Some(mut element) = request_layout.trigger_element.take() {
                    element.paint(window, cx);
                }

                if let Some(mut element) = request_layout.popover_element.take() {
                    element.paint(window, cx);
                    return;
                }

                // When mouse click down in the trigger bounds, open the popover.
                let Some(content_build) = this.content.take() else {
                    return;
                };
                let old_content_view = element_state.content_view.clone();
                let hitbox_id = prepaint.hitbox.id;
                let mouse_button = this.mouse_button;
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == mouse_button
                        && hitbox_id.is_hovered(window)
                    {
                        cx.stop_propagation();
                        window.prevent_default();

                        let new_content_view = (content_build)(window, cx);
                        let old_content_view1 = old_content_view.clone();

                        let previous_focus_handle = window.focused(cx);

                        window
                            .subscribe(
                                &new_content_view,
                                cx,
                                move |modal, _: &DismissEvent, window, cx| {
                                    if modal.focus_handle(cx).contains_focused(window, cx) {
                                        if let Some(previous_focus_handle) =
                                            previous_focus_handle.as_ref()
                                        {
                                            window.focus(previous_focus_handle);
                                        }
                                    }
                                    *old_content_view1.borrow_mut() = None;

                                    window.refresh();
                                },
                            )
                            .detach();

                        window.focus(&new_content_view.focus_handle(cx));
                        *old_content_view.borrow_mut() = Some(new_content_view);
                        window.refresh();
                    }
                });
            },
        );
    }
}
