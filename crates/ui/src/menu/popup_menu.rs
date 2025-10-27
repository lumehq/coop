use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    anchored, canvas, div, px, rems, Action, AnyElement, App, AppContext, Bounds, Context, Corner,
    DismissEvent, Edges, Entity, EventEmitter, FocusHandle, Focusable, Half, InteractiveElement,
    IntoElement, KeyBinding, MouseDownEvent, OwnedMenuItem, ParentElement, Pixels, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Subscription, WeakEntity,
    Window,
};
use theme::ActiveTheme;

use crate::actions::{Cancel, Confirm, SelectDown, SelectLeft, SelectRight, SelectUp};
use crate::button::Button;
use crate::menu::menu_item::MenuItemElement;
use crate::popover::Popover;
use crate::scroll::{Scrollbar, ScrollbarState};
use crate::{h_flex, v_flex, Icon, IconName, Kbd, Selectable, Side, Sizable as _, Size, StyledExt};

const CONTEXT: &str = "PopupMenu";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("right", SelectRight, Some(CONTEXT)),
    ]);
}

pub trait PopupMenuExt: Styled + Selectable + InteractiveElement + IntoElement + 'static {
    /// Create a popup menu with the given items, anchored to the TopLeft corner
    fn popup_menu(
        self,
        f: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Popover<PopupMenu> {
        self.popup_menu_with_anchor(Corner::TopLeft, f)
    }

    /// Create a popup menu with the given items, anchored to the given corner
    fn popup_menu_with_anchor(
        mut self,
        anchor: impl Into<Corner>,
        f: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Popover<PopupMenu> {
        let style = self.style().clone();
        let id = self.interactivity().element_id.clone();

        Popover::new(SharedString::from(format!("popup-menu:{id:?}")))
            .no_style()
            .trigger(self)
            .trigger_style(style)
            .anchor(anchor.into())
            .content(move |window, cx| {
                PopupMenu::build(window, cx, |menu, window, cx| f(menu, window, cx))
            })
    }
}
impl PopupMenuExt for Button {}

#[allow(clippy::type_complexity)]
pub(crate) enum PopupMenuItem {
    Separator,
    Label(SharedString),
    Item {
        icon: Option<Icon>,
        label: SharedString,
        disabled: bool,
        is_link: bool,
        action: Option<Box<dyn Action>>,
        // For link item
        handler: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    },
    ElementItem {
        icon: Option<Icon>,
        disabled: bool,
        action: Box<dyn Action>,
        render: Box<dyn Fn(&mut Window, &mut App) -> AnyElement + 'static>,
        handler: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    },
    Submenu {
        icon: Option<Icon>,
        label: SharedString,
        disabled: bool,
        menu: Entity<PopupMenu>,
    },
}

impl PopupMenuItem {
    #[inline]
    fn is_clickable(&self) -> bool {
        !matches!(self, PopupMenuItem::Separator)
            && matches!(
                self,
                PopupMenuItem::Item {
                    disabled: false,
                    ..
                } | PopupMenuItem::ElementItem {
                    disabled: false,
                    ..
                } | PopupMenuItem::Submenu {
                    disabled: false,
                    ..
                }
            )
    }

    #[inline]
    fn is_separator(&self) -> bool {
        matches!(self, PopupMenuItem::Separator)
    }
}

pub struct PopupMenu {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) menu_items: Vec<PopupMenuItem>,
    /// The focus handle of Entity to handle actions.
    pub(crate) action_context: Option<FocusHandle>,
    has_icon: bool,
    selected_index: Option<usize>,
    min_width: Option<Pixels>,
    max_width: Option<Pixels>,
    max_height: Option<Pixels>,
    bounds: Bounds<Pixels>,
    size: Size,

    /// The parent menu of this menu, if this is a submenu
    parent_menu: Option<WeakEntity<Self>>,
    scrollable: bool,
    external_link_icon: bool,
    scroll_handle: ScrollHandle,
    scroll_state: ScrollbarState,
    // This will update on render
    submenu_anchor: (Corner, Pixels),

    _subscriptions: Vec<Subscription>,
}

impl PopupMenu {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            action_context: None,
            parent_menu: None,
            menu_items: Vec::new(),
            selected_index: None,
            min_width: None,
            max_width: None,
            max_height: None,
            has_icon: false,
            bounds: Bounds::default(),
            scrollable: false,
            scroll_handle: ScrollHandle::default(),
            scroll_state: ScrollbarState::default(),
            external_link_icon: true,
            size: Size::default(),
            submenu_anchor: (Corner::TopLeft, Pixels::ZERO),
            _subscriptions: vec![],
        }
    }

    pub fn build(
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(Self, &mut Window, &mut Context<PopupMenu>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| f(Self::new(cx), window, cx))
    }

    /// Set the focus handle of Entity to handle actions.
    ///
    /// When the menu is dismissed or before an action is triggered, the focus will be returned to this handle.
    ///
    /// Then the action will be dispatched to this handle.
    pub fn action_context(mut self, handle: FocusHandle) -> Self {
        self.action_context = Some(handle);
        self
    }

    /// Set min width of the popup menu, default is 120px
    pub fn min_w(mut self, width: impl Into<Pixels>) -> Self {
        self.min_width = Some(width.into());
        self
    }

    /// Set max width of the popup menu, default is 500px
    pub fn max_w(mut self, width: impl Into<Pixels>) -> Self {
        self.max_width = Some(width.into());
        self
    }

    /// Set max height of the popup menu, default is half of the window height
    pub fn max_h(mut self, height: impl Into<Pixels>) -> Self {
        self.max_height = Some(height.into());
        self
    }

    /// Set the menu to be scrollable to show vertical scrollbar.
    ///
    /// NOTE: If this is true, the sub-menus will cannot be support.
    pub fn scrollable(mut self) -> Self {
        self.scrollable = true;
        self
    }

    /// Set the menu to show external link icon, default is true.
    pub fn external_link_icon(mut self, visible: bool) -> Self {
        self.external_link_icon = visible;
        self
    }

    /// Add Menu Item
    pub fn menu(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.menu_with_disabled(label, action, false)
    }

    /// Add Menu Item with enable state
    pub fn menu_with_enable(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        enable: bool,
    ) -> Self {
        self.add_menu_item(label, None, action, !enable);
        self
    }

    /// Add Menu Item with disabled state
    pub fn menu_with_disabled(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        disabled: bool,
    ) -> Self {
        self.add_menu_item(label, None, action, disabled);
        self
    }

    /// Add label
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.menu_items.push(PopupMenuItem::Label(label.into()));
        self
    }

    /// Add Menu to open link
    pub fn link(self, label: impl Into<SharedString>, href: impl Into<String>) -> Self {
        self.link_with_disabled(label, href, false)
    }

    /// Add Menu to open link with disabled state
    pub fn link_with_disabled(
        mut self,
        label: impl Into<SharedString>,
        href: impl Into<String>,
        disabled: bool,
    ) -> Self {
        let href = href.into();
        self.menu_items.push(PopupMenuItem::Item {
            icon: None,
            label: label.into(),
            disabled,
            action: None,
            is_link: true,
            handler: Some(Rc::new(move |_, cx| cx.open_url(&href))),
        });
        self
    }

    /// Add Menu to open link
    pub fn link_with_icon(
        self,
        label: impl Into<SharedString>,
        icon: impl Into<Icon>,
        href: impl Into<String>,
    ) -> Self {
        self.link_with_icon_and_disabled(label, icon, href, false)
    }

    /// Add Menu to open link with icon and disabled state
    pub fn link_with_icon_and_disabled(
        mut self,
        label: impl Into<SharedString>,
        icon: impl Into<Icon>,
        href: impl Into<String>,
        disabled: bool,
    ) -> Self {
        let href = href.into();
        self.menu_items.push(PopupMenuItem::Item {
            icon: Some(icon.into()),
            label: label.into(),
            disabled,
            action: None,
            is_link: true,
            handler: Some(Rc::new(move |_, cx| cx.open_url(&href))),
        });
        self
    }

    /// Add Menu Item with Icon.
    pub fn menu_with_icon(
        self,
        label: impl Into<SharedString>,
        icon: impl Into<Icon>,
        action: Box<dyn Action>,
    ) -> Self {
        self.menu_with_icon_and_disabled(label, icon, action, false)
    }

    /// Add Menu Item with Icon and disabled state
    pub fn menu_with_icon_and_disabled(
        mut self,
        label: impl Into<SharedString>,
        icon: impl Into<Icon>,
        action: Box<dyn Action>,
        disabled: bool,
    ) -> Self {
        self.add_menu_item(label, Some(icon.into()), action, disabled);
        self
    }

    /// Add Menu Item with check icon
    pub fn menu_with_check(
        self,
        label: impl Into<SharedString>,
        checked: bool,
        action: Box<dyn Action>,
    ) -> Self {
        self.menu_with_check_and_disabled(label, checked, action, false)
    }

    /// Add Menu Item with check icon and disabled state
    pub fn menu_with_check_and_disabled(
        mut self,
        label: impl Into<SharedString>,
        checked: bool,
        action: Box<dyn Action>,
        disabled: bool,
    ) -> Self {
        if checked {
            self.add_menu_item(label, Some(IconName::Check.into()), action, disabled);
        } else {
            self.add_menu_item(label, None, action, disabled);
        }

        self
    }

    /// Add Menu Item with custom element render.
    pub fn menu_element<F, E>(self, action: Box<dyn Action>, builder: F) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        self.menu_element_with_check(false, action, builder)
    }

    /// Add Menu Item with custom element render with disabled state.
    pub fn menu_element_with_disabled<F, E>(
        self,
        action: Box<dyn Action>,
        disabled: bool,
        builder: F,
    ) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        self.menu_element_with_check_and_disabled(false, action, disabled, builder)
    }

    /// Add Menu Item with custom element render with icon.
    pub fn menu_element_with_icon<F, E>(
        self,
        icon: impl Into<Icon>,
        action: Box<dyn Action>,
        builder: F,
    ) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        self.menu_element_with_icon_and_disabled(icon, action, false, builder)
    }

    /// Add Menu Item with custom element render with icon and disabled state
    pub fn menu_element_with_icon_and_disabled<F, E>(
        mut self,
        icon: impl Into<Icon>,
        action: Box<dyn Action>,
        disabled: bool,
        builder: F,
    ) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        self.menu_items.push(PopupMenuItem::ElementItem {
            render: Box::new(move |window, cx| builder(window, cx).into_any_element()),
            action,
            icon: Some(icon.into()),
            disabled,
            handler: None,
        });
        self.has_icon = true;
        self
    }

    /// Add Menu Item with custom element render with check state
    pub fn menu_element_with_check<F, E>(
        self,
        checked: bool,
        action: Box<dyn Action>,
        builder: F,
    ) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        self.menu_element_with_check_and_disabled(checked, action, false, builder)
    }

    /// Add Menu Item with custom element render with check state and disabled state
    pub fn menu_element_with_check_and_disabled<F, E>(
        mut self,
        checked: bool,
        action: Box<dyn Action>,
        disabled: bool,
        builder: F,
    ) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        if checked {
            self.menu_items.push(PopupMenuItem::ElementItem {
                render: Box::new(move |window, cx| builder(window, cx).into_any_element()),
                action,
                handler: None,
                icon: Some(IconName::Check.into()),
                disabled,
            });
            self.has_icon = true;
        } else {
            self.menu_items.push(PopupMenuItem::ElementItem {
                render: Box::new(move |window, cx| builder(window, cx).into_any_element()),
                action,
                handler: None,
                icon: None,
                disabled,
            });
        }
        self
    }

    /// Use small size, the menu item will have smaller height.
    #[allow(dead_code)]
    pub(crate) fn small(mut self) -> Self {
        self.size = Size::Small;
        self
    }

    /// Add a separator Menu Item
    pub fn separator(mut self) -> Self {
        if self.menu_items.is_empty() {
            return self;
        }

        if let Some(PopupMenuItem::Separator) = self.menu_items.last() {
            return self;
        }

        self.menu_items.push(PopupMenuItem::Separator);
        self
    }

    /// Add a Submenu
    pub fn submenu(
        self,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.submenu_with_icon(None, label, window, cx, f)
    }

    /// Add a Submenu item with disabled state
    pub fn submenu_with_disabled(
        self,
        label: impl Into<SharedString>,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.submenu_with_icon_with_disabled(None, label, disabled, window, cx, f)
    }

    /// Add a Submenu item with icon
    pub fn submenu_with_icon(
        self,
        icon: Option<Icon>,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.submenu_with_icon_with_disabled(icon, label, false, window, cx, f)
    }

    /// Add a Submenu item with icon and disabled state
    pub fn submenu_with_icon_with_disabled(
        mut self,
        icon: Option<Icon>,
        label: impl Into<SharedString>,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        let submenu = PopupMenu::build(window, cx, f);
        let parent_menu = cx.entity().downgrade();
        submenu.update(cx, |view, _| {
            view.parent_menu = Some(parent_menu);
        });

        self.menu_items.push(PopupMenuItem::Submenu {
            icon,
            label: label.into(),
            menu: submenu,
            disabled,
        });
        self
    }

    fn add_menu_item(
        &mut self,
        label: impl Into<SharedString>,
        icon: Option<Icon>,
        action: Box<dyn Action>,
        disabled: bool,
    ) -> &mut Self {
        if icon.is_some() {
            self.has_icon = true;
        }

        self.menu_items.push(PopupMenuItem::Item {
            icon,
            label: label.into(),
            disabled,
            action: Some(action.boxed_clone()),
            is_link: false,
            handler: None,
        });
        self
    }

    pub(super) fn with_menu_items<I>(
        mut self,
        items: impl IntoIterator<Item = I>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        I: Into<OwnedMenuItem>,
    {
        for item in items {
            match item.into() {
                OwnedMenuItem::Action { name, action, .. } => {
                    self = self.menu(name, action.boxed_clone())
                }
                OwnedMenuItem::Separator => {
                    self = self.separator();
                }
                OwnedMenuItem::Submenu(submenu) => {
                    self = self.submenu(submenu.name, window, cx, move |menu, window, cx| {
                        menu.with_menu_items(submenu.items.clone(), window, cx)
                    })
                }
                OwnedMenuItem::SystemMenu(_) => {}
            }
        }

        if self.menu_items.len() > 20 {
            self.scrollable = true;
        }

        self
    }

    pub(crate) fn active_submenu(&self) -> Option<Entity<PopupMenu>> {
        if let Some(ix) = self.selected_index {
            if let Some(item) = self.menu_items.get(ix) {
                return match item {
                    PopupMenuItem::Submenu { menu, .. } => Some(menu.clone()),
                    _ => None,
                };
            }
        }

        None
    }

    pub fn is_empty(&self) -> bool {
        self.menu_items.is_empty()
    }

    fn clickable_menu_items(&self) -> impl Iterator<Item = (usize, &PopupMenuItem)> {
        self.menu_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_clickable())
    }

    fn on_click(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        window.prevent_default();
        self.selected_index = Some(ix);
        self.confirm(&Confirm { secondary: false }, window, cx);
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_index {
            let item = self.menu_items.get(index);

            match item {
                Some(PopupMenuItem::Item {
                    handler, action, ..
                }) => {
                    if let Some(handler) = handler {
                        handler(window, cx);
                    } else if let Some(action) = action.as_ref() {
                        self.dispatch_confirm_action(action.as_ref(), window, cx);
                    }

                    self.dismiss(&Cancel, window, cx)
                }
                Some(PopupMenuItem::ElementItem {
                    handler, action, ..
                }) => {
                    if let Some(handler) = handler {
                        handler(window, cx);
                    } else {
                        self.dispatch_confirm_action(action.as_ref(), window, cx);
                    }
                    self.dismiss(&Cancel, window, cx)
                }
                _ => {}
            }
        }
    }

    fn dispatch_confirm_action(
        &self,
        action: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(context) = self.action_context.as_ref() {
            context.focus(window);
        }

        window.dispatch_action(action.boxed_clone(), cx);
    }

    fn set_selected_index(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.selected_index != Some(ix) {
            self.selected_index = Some(ix);
            self.scroll_handle.scroll_to_item(ix);
            cx.notify();
        }
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let ix = self.selected_index.unwrap_or(0);

        if let Some((prev_ix, _)) = self
            .menu_items
            .iter()
            .enumerate()
            .rev()
            .find(|(i, item)| *i < ix && item.is_clickable())
        {
            self.set_selected_index(prev_ix, cx);
            return;
        }

        let last_clickable_ix = self.clickable_menu_items().last().map(|(ix, _)| ix);
        self.set_selected_index(last_clickable_ix.unwrap_or(0), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let Some(ix) = self.selected_index else {
            self.set_selected_index(0, cx);
            return;
        };

        if let Some((next_ix, _)) = self
            .menu_items
            .iter()
            .enumerate()
            .find(|(i, item)| *i > ix && item.is_clickable())
        {
            self.set_selected_index(next_ix, cx);
            return;
        }

        self.set_selected_index(0, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let handled = if matches!(self.submenu_anchor.0, Corner::TopLeft | Corner::BottomLeft) {
            self._unselect_submenu(window, cx)
        } else {
            self._select_submenu(window, cx)
        };

        if self.parent_side(cx).is_left() {
            self._focus_parent_menu(window, cx);
        }

        if handled {
            return;
        }

        // For parent AppMenuBar to handle.
        if self.parent_menu.is_none() {
            cx.propagate();
        }
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        let handled = if matches!(self.submenu_anchor.0, Corner::TopLeft | Corner::BottomLeft) {
            self._select_submenu(window, cx)
        } else {
            self._unselect_submenu(window, cx)
        };

        if self.parent_side(cx).is_right() {
            self._focus_parent_menu(window, cx);
        }

        if handled {
            return;
        }

        // For parent AppMenuBar to handle.
        if self.parent_menu.is_none() {
            cx.propagate();
        }
    }

    fn _select_submenu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(active_submenu) = self.active_submenu() {
            // Focus the submenu, so that can be handle the action.
            active_submenu.update(cx, |view, cx| {
                view.set_selected_index(0, cx);
                view.focus_handle.focus(window);
            });
            cx.notify();
            return true;
        }

        false
    }

    fn _unselect_submenu(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(active_submenu) = self.active_submenu() {
            active_submenu.update(cx, |view, cx| {
                view.selected_index = None;
                cx.notify();
            });
            return true;
        }

        false
    }

    fn _focus_parent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(parent) = self.parent_menu.as_ref() else {
            return;
        };
        let Some(parent) = parent.upgrade() else {
            return;
        };

        self.selected_index = None;
        parent.update(cx, |view, cx| {
            view.focus_handle.focus(window);
            cx.notify();
        });
    }

    fn parent_side(&self, cx: &App) -> Side {
        let Some(parent) = self.parent_menu.as_ref() else {
            return Side::Left;
        };

        let Some(parent) = parent.upgrade() else {
            return Side::Left;
        };

        match parent.read(cx).submenu_anchor.0 {
            Corner::TopLeft | Corner::BottomLeft => Side::Left,
            Corner::TopRight | Corner::BottomRight => Side::Right,
        }
    }

    fn dismiss(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_submenu().is_some() {
            return;
        }

        cx.emit(DismissEvent);

        // Focus back to the previous focused handle.
        if let Some(action_context) = self.action_context.as_ref() {
            window.focus(action_context);
        }

        let Some(parent_menu) = self.parent_menu.clone() else {
            return;
        };

        // Dismiss parent menu, when this menu is dismissed
        _ = parent_menu.update(cx, |view, cx| {
            view.selected_index = None;
            view.dismiss(&Cancel, window, cx);
        });
    }

    fn render_key_binding(
        &self,
        action: Option<Box<dyn Action>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let action = action?;

        match self
            .action_context
            .as_ref()
            .and_then(|handle| Kbd::binding_for_action_in(action.as_ref(), handle, window))
        {
            Some(kbd) => Some(kbd),
            // Fallback to App level key binding
            None => Kbd::binding_for_action(action.as_ref(), None, window),
        }
        .map(|this| {
            this.p_0()
                .flex_nowrap()
                .border_0()
                .bg(gpui::transparent_white())
        })
    }

    fn render_icon(
        has_icon: bool,
        icon: Option<Icon>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !has_icon {
            return None;
        }

        let icon = h_flex()
            .w_3p5()
            .h_3p5()
            .justify_center()
            .text_sm()
            .when_some(icon, |this, icon| this.child(icon.clone().xsmall()));

        Some(icon)
    }

    #[inline]
    fn max_width(&self) -> Pixels {
        self.max_width.unwrap_or(px(500.))
    }

    /// Calculate the anchor corner and left offset for child submenu
    fn update_submenu_menu_anchor(&mut self, window: &Window) {
        let bounds = self.bounds;
        let max_width = self.max_width();
        let (anchor, left) = if max_width + bounds.origin.x > window.bounds().size.width {
            (Corner::TopRight, -px(16.))
        } else {
            (Corner::TopLeft, bounds.size.width - px(8.))
        };

        let is_bottom_pos = bounds.origin.y + bounds.size.height > window.bounds().size.height;
        self.submenu_anchor = if is_bottom_pos {
            (anchor.other_side_corner_along(gpui::Axis::Vertical), left)
        } else {
            (anchor, left)
        };
    }

    fn render_item(
        &self,
        ix: usize,
        item: &PopupMenuItem,
        state: ItemState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_icon = self.has_icon;
        let selected = self.selected_index == Some(ix);
        const EDGE_PADDING: Pixels = px(4.);
        const INNER_PADDING: Pixels = px(8.);

        let is_submenu = matches!(item, PopupMenuItem::Submenu { .. });
        let group_name = format!("popup-menu-item-{ix}");

        let (item_height, radius) = match self.size {
            Size::Small => (px(20.), state.radius.half()),
            _ => (px(26.), state.radius),
        };

        let this = MenuItemElement::new(ix, &group_name)
            .relative()
            .text_sm()
            .py_0()
            .px(INNER_PADDING)
            .rounded(radius)
            .items_center()
            .selected(selected)
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.selected_index = Some(ix);
                } else if !is_submenu && this.selected_index == Some(ix) {
                    // TODO: Better handle the submenu unselection when hover out
                    this.selected_index = None;
                }

                cx.notify();
            }));

        match item {
            PopupMenuItem::Separator => this
                .h_auto()
                .p_0()
                .my_0p5()
                .mx_neg_1()
                .h(px(1.))
                .bg(cx.theme().border)
                .disabled(true),
            PopupMenuItem::Label(label) => this.disabled(true).cursor_default().child(
                h_flex()
                    .cursor_default()
                    .items_center()
                    .gap_x_1()
                    .font_semibold()
                    .children(Self::render_icon(has_icon, None, window, cx))
                    .child(label.clone()),
            ),
            PopupMenuItem::ElementItem {
                render,
                icon,
                disabled,
                ..
            } => this
                .when(!disabled, |this| {
                    this.on_click(
                        cx.listener(move |this, _, window, cx| this.on_click(ix, window, cx)),
                    )
                })
                .disabled(*disabled)
                .child(
                    h_flex()
                        .flex_1()
                        .min_h(item_height)
                        .items_center()
                        .gap_x_1()
                        .children(Self::render_icon(has_icon, icon.clone(), window, cx))
                        .child((render)(window, cx)),
                ),
            PopupMenuItem::Item {
                icon,
                label,
                action,
                disabled,
                is_link,
                ..
            } => {
                let show_link_icon = *is_link && self.external_link_icon;
                let action = action.as_ref().map(|action| action.boxed_clone());
                let key = self.render_key_binding(action, window, cx);

                this.when(!disabled, |this| {
                    this.on_click(
                        cx.listener(move |this, _, window, cx| this.on_click(ix, window, cx)),
                    )
                })
                .disabled(*disabled)
                .h(item_height)
                .children(Self::render_icon(has_icon, icon.clone(), window, cx))
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_center()
                        .justify_between()
                        .when(!show_link_icon, |this| this.child(label.clone()))
                        .when(show_link_icon, |this| {
                            this.child(
                                h_flex()
                                    .w_full()
                                    .justify_between()
                                    .gap_1p5()
                                    .child(label.clone())
                                    .child(
                                        Icon::new(IconName::OpenUrl)
                                            .xsmall()
                                            .text_color(cx.theme().text_muted),
                                    ),
                            )
                        })
                        .children(key),
                )
            }
            PopupMenuItem::Submenu {
                icon,
                label,
                menu,
                disabled,
            } => this
                .selected(selected)
                .disabled(*disabled)
                .items_start()
                .child(
                    h_flex()
                        .min_h(item_height)
                        .size_full()
                        .items_center()
                        .gap_x_1()
                        .children(Self::render_icon(has_icon, icon.clone(), window, cx))
                        .child(
                            h_flex()
                                .flex_1()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(label.clone())
                                .child(IconName::CaretRight),
                        ),
                )
                .when(selected, |this| {
                    this.child({
                        let (anchor, left) = self.submenu_anchor;
                        let is_bottom_pos =
                            matches!(anchor, Corner::BottomLeft | Corner::BottomRight);
                        anchored()
                            .anchor(anchor)
                            .child(
                                div()
                                    .id("submenu")
                                    .occlude()
                                    .when(is_bottom_pos, |this| this.bottom_0())
                                    .when(!is_bottom_pos, |this| this.top_neg_1())
                                    .left(left)
                                    .child(menu.clone()),
                            )
                            .snap_to_window_with_margin(Edges::all(EDGE_PADDING))
                    })
                }),
        }
    }
}

impl FluentBuilder for PopupMenu {}
impl EventEmitter<DismissEvent> for PopupMenu {}
impl Focusable for PopupMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Clone, Copy)]
struct ItemState {
    radius: Pixels,
}

impl Render for PopupMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_submenu_menu_anchor(window);

        let max_width = self.max_width();
        let max_height = self.max_height.map_or_else(
            || {
                let window_half_height = window.window_bounds().get_bounds().size.height * 0.5;
                window_half_height.min(px(450.))
            },
            |height| height,
        );

        let view = cx.entity().clone();
        let items_count = self.menu_items.len();
        let item_state = ItemState {
            radius: cx.theme().radius.min(px(8.)),
        };

        v_flex()
            .id("popup-menu")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down_out(cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                // Do not dismiss, if click inside the parent menu
                if let Some(parent) = this.parent_menu.as_ref() {
                    if let Some(parent) = parent.upgrade() {
                        if parent.read(cx).bounds.contains(&ev.position) {
                            return;
                        }
                    }
                }

                this.dismiss(&Cancel, window, cx);
            }))
            .popover_style(cx)
            .text_color(cx.theme().text)
            .relative()
            .child(
                v_flex()
                    .id("items")
                    .p_1()
                    .gap_y_0p5()
                    .min_w(rems(8.))
                    .when_some(self.min_width, |this, min_width| this.min_w(min_width))
                    .max_w(max_width)
                    .when(self.scrollable, |this| {
                        this.max_h(max_height)
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                    })
                    .children(
                        self.menu_items
                            .iter()
                            .enumerate()
                            // Ignore last separator
                            .filter(|(ix, item)| !(*ix + 1 == items_count && item.is_separator()))
                            .map(|(ix, item)| self.render_item(ix, item, item_state, window, cx)),
                    )
                    .child({
                        canvas(
                            move |bounds, _, cx| view.update(cx, |r, _| r.bounds = bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    }),
            )
            .when(self.scrollable, |this| {
                // TODO: When the menu is limited by `overflow_y_scroll`, the sub-menu will cannot be displayed.
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(Scrollbar::vertical(&self.scroll_state, &self.scroll_handle)),
                )
            })
    }
}
