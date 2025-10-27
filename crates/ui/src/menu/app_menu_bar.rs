use gpui::prelude::FluentBuilder;
use gpui::{
    anchored, deferred, div, px, App, AppContext as _, ClickEvent, Context, DismissEvent, Entity,
    Focusable, InteractiveElement as _, IntoElement, KeyBinding, OwnedMenu, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
};

use crate::actions::{Cancel, SelectLeft, SelectRight};
use crate::button::{Button, ButtonVariants};
use crate::popup_menu::PopupMenu;
use crate::{h_flex, Selectable, Sizable};

const CONTEXT: &str = "AppMenuBar";
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("right", SelectRight, Some(CONTEXT)),
    ]);
}

/// The application menu bar, for Windows and Linux.
pub struct AppMenuBar {
    menus: Vec<Entity<AppMenu>>,
    selected_ix: Option<usize>,
}

impl AppMenuBar {
    /// Create a new app menu bar.
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let menu_bar = cx.entity();
            let menus = cx
                .get_menus()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(ix, menu)| AppMenu::new(ix, menu, menu_bar.clone(), window, cx))
                .collect();

            Self {
                selected_ix: None,
                menus,
            }
        })
    }

    fn move_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected_ix) = self.selected_ix else {
            return;
        };

        let new_ix = if selected_ix == 0 {
            self.menus.len().saturating_sub(1)
        } else {
            selected_ix.saturating_sub(1)
        };
        self.set_selected_ix(Some(new_ix), window, cx);
    }

    fn move_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected_ix) = self.selected_ix else {
            return;
        };

        let new_ix = if selected_ix + 1 >= self.menus.len() {
            0
        } else {
            selected_ix + 1
        };
        self.set_selected_ix(Some(new_ix), window, cx);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.set_selected_ix(None, window, cx);
    }

    fn set_selected_ix(&mut self, ix: Option<usize>, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_ix = ix;
        cx.notify();
    }

    #[inline]
    fn has_activated_menu(&self) -> bool {
        self.selected_ix.is_some()
    }
}

impl Render for AppMenuBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("app-menu-bar")
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::cancel))
            .size_full()
            .gap_x_1()
            .overflow_x_scroll()
            .children(self.menus.clone())
    }
}

/// A menu in the menu bar.
pub(super) struct AppMenu {
    menu_bar: Entity<AppMenuBar>,
    ix: usize,
    name: SharedString,
    menu: OwnedMenu,
    popup_menu: Option<Entity<PopupMenu>>,

    _subscription: Option<Subscription>,
}

impl AppMenu {
    pub(super) fn new(
        ix: usize,
        menu: &OwnedMenu,
        menu_bar: Entity<AppMenuBar>,
        _: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let name = menu.name.clone();
        cx.new(|_| Self {
            ix,
            menu_bar,
            name,
            menu: menu.clone(),
            popup_menu: None,
            _subscription: None,
        })
    }

    fn build_popup_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<PopupMenu> {
        let popup_menu = match self.popup_menu.as_ref() {
            None => {
                let items = self.menu.items.clone();
                let popup_menu = PopupMenu::build(window, cx, |menu, window, cx| {
                    menu.when_some(window.focused(cx), |this, handle| {
                        this.action_context(handle)
                    })
                    .with_menu_items(items, window, cx)
                });
                popup_menu.read(cx).focus_handle(cx).focus(window);
                self._subscription =
                    Some(cx.subscribe_in(&popup_menu, window, Self::handle_dismiss));
                self.popup_menu = Some(popup_menu.clone());

                popup_menu
            }
            Some(menu) => menu.clone(),
        };

        let focus_handle = popup_menu.read(cx).focus_handle(cx);
        if !focus_handle.contains_focused(window, cx) {
            focus_handle.focus(window);
        }

        popup_menu
    }

    fn handle_dismiss(
        &mut self,
        _: &Entity<PopupMenu>,
        _: &DismissEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscription.take();
        self.popup_menu.take();
        self.menu_bar.update(cx, |state, cx| {
            state.cancel(&Cancel, window, cx);
        });
    }

    fn handle_trigger_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_selected = self.menu_bar.read(cx).selected_ix == Some(self.ix);

        self.menu_bar.update(cx, |state, cx| {
            let new_ix = if is_selected { None } else { Some(self.ix) };
            state.set_selected_ix(new_ix, window, cx);
        });
    }

    fn handle_hover(&mut self, hovered: &bool, window: &mut Window, cx: &mut Context<Self>) {
        if !*hovered {
            return;
        }

        let has_activated_menu = self.menu_bar.read(cx).has_activated_menu();
        if !has_activated_menu {
            return;
        }

        self.menu_bar.update(cx, |state, cx| {
            state.set_selected_ix(Some(self.ix), window, cx);
        });
    }
}

impl Render for AppMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_bar = self.menu_bar.read(cx);
        let is_selected = menu_bar.selected_ix == Some(self.ix);

        div()
            .id(self.ix)
            .relative()
            .child(
                Button::new("menu")
                    .small()
                    .py_0p5()
                    .xsmall()
                    .ghost()
                    .label(self.name.clone())
                    .selected(is_selected)
                    .on_click(cx.listener(Self::handle_trigger_click)),
            )
            .on_hover(cx.listener(Self::handle_hover))
            .when(is_selected, |this| {
                this.child(deferred(
                    anchored()
                        .anchor(gpui::Corner::TopLeft)
                        .snap_to_window_with_margin(px(8.))
                        .child(
                            div()
                                .size_full()
                                .occlude()
                                .top_1()
                                .child(self.build_popup_menu(window, cx)),
                        ),
                ))
            })
    }
}
