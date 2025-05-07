use gpui::{
    actions, anchored, canvas, deferred, div, prelude::FluentBuilder, px, rems, AnyElement, App,
    AppContext, Bounds, ClickEvent, Context, DismissEvent, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, Length, ParentElement,
    Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    WeakEntity, Window,
};
use theme::ActiveTheme;

use crate::{
    h_flex,
    list::{self, List, ListDelegate, ListItem},
    v_flex, Icon, IconName, Sizable, Size, StyleSized, StyledExt,
};

actions!(dropdown, [Up, Down, Enter, Escape]);

const CONTEXT: &str = "Dropdown";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", Up, Some(CONTEXT)),
        KeyBinding::new("down", Down, Some(CONTEXT)),
        KeyBinding::new("enter", Enter, Some(CONTEXT)),
        KeyBinding::new("escape", Escape, Some(CONTEXT)),
    ])
}

/// A trait for items that can be displayed in a dropdown.
pub trait DropdownItem {
    type Value: Clone;
    fn title(&self) -> SharedString;
    fn value(&self) -> &Self::Value;
}

impl DropdownItem for String {
    type Value = Self;

    fn title(&self) -> SharedString {
        SharedString::from(self.to_string())
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl DropdownItem for SharedString {
    type Value = Self;

    fn title(&self) -> SharedString {
        SharedString::from(self.to_string())
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

pub trait DropdownDelegate: Sized {
    type Item: DropdownItem;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, ix: usize) -> Option<&Self::Item>;

    fn position<V>(&self, value: &V) -> Option<usize>
    where
        Self::Item: DropdownItem<Value = V>,
        V: PartialEq,
    {
        (0..self.len()).find(|&i| self.get(i).is_some_and(|item| item.value() == value))
    }

    fn can_search(&self) -> bool {
        false
    }

    fn perform_search(
        &mut self,
        _query: &str,
        _window: &mut Window,
        _cx: &mut Context<Dropdown<Self>>,
    ) -> Task<()> {
        Task::ready(())
    }
}

impl<T: DropdownItem> DropdownDelegate for Vec<T> {
    type Item = T;

    fn len(&self) -> usize {
        self.len()
    }

    fn get(&self, ix: usize) -> Option<&Self::Item> {
        self.as_slice().get(ix)
    }

    fn position<V>(&self, value: &V) -> Option<usize>
    where
        Self::Item: DropdownItem<Value = V>,
        V: PartialEq,
    {
        self.iter().position(|v| v.value() == value)
    }
}

struct DropdownListDelegate<D: DropdownDelegate + 'static> {
    delegate: D,
    dropdown: WeakEntity<Dropdown<D>>,
    selected_index: Option<usize>,
}

impl<D> ListDelegate for DropdownListDelegate<D>
where
    D: DropdownDelegate + 'static,
{
    type Item = ListItem;

    fn items_count(&self, _: &App) -> usize {
        self.delegate.len()
    }

    fn confirmed_index(&self, _: &App) -> Option<usize> {
        self.selected_index
    }

    fn render_item(
        &self,
        ix: usize,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<List<Self>>,
    ) -> Option<Self::Item> {
        let selected = self.selected_index == Some(ix);
        let size = self
            .dropdown
            .upgrade()
            .map_or(Size::Medium, |dropdown| dropdown.read(cx).size);

        if let Some(item) = self.delegate.get(ix) {
            let list_item = ListItem::new(("list-item", ix))
                .check_icon(IconName::Check)
                .cursor_pointer()
                .selected(selected)
                .input_text_size(size)
                .list_size(size)
                .child(div().whitespace_nowrap().child(item.title().to_string()));
            Some(list_item)
        } else {
            None
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<List<Self>>) {
        let dropdown = self.dropdown.clone();
        cx.defer_in(window, move |_, window, cx| {
            _ = dropdown.update(cx, |this, cx| {
                this.open = false;
                this.focus(window, cx);
            });
        });
    }

    fn confirm(&mut self, ix: Option<usize>, window: &mut Window, cx: &mut Context<List<Self>>) {
        self.selected_index = ix;

        let selected_value = self
            .selected_index
            .and_then(|ix| self.delegate.get(ix))
            .map(|item| item.value().clone());
        let dropdown = self.dropdown.clone();

        cx.defer_in(window, move |_, window, cx| {
            _ = dropdown.update(cx, |this, cx| {
                cx.emit(DropdownEvent::Confirm(selected_value.clone()));
                this.selected_value = selected_value;
                this.open = false;
                this.focus(window, cx);
            });
        });
    }

    fn perform_search(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<List<Self>>,
    ) -> Task<()> {
        self.dropdown.upgrade().map_or(Task::ready(()), |dropdown| {
            dropdown.update(cx, |_, cx| self.delegate.perform_search(query, window, cx))
        })
    }

    fn set_selected_index(
        &mut self,
        ix: Option<usize>,
        _: &mut Window,
        _: &mut Context<List<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn render_empty(&self, window: &mut Window, cx: &mut Context<List<Self>>) -> impl IntoElement {
        if let Some(empty) = self
            .dropdown
            .upgrade()
            .and_then(|dropdown| dropdown.read(cx).empty.as_ref())
        {
            empty(window, cx).into_any_element()
        } else {
            h_flex()
                .justify_center()
                .py_6()
                .text_color(cx.theme().text_muted)
                .child(Icon::new(IconName::Inbox).size(px(28.)))
                .into_any_element()
        }
    }
}

pub enum DropdownEvent<D: DropdownDelegate + 'static> {
    Confirm(Option<<D::Item as DropdownItem>::Value>),
}

type Empty = Option<Box<dyn Fn(&Window, &App) -> AnyElement + 'static>>;

/// A Dropdown element.
pub struct Dropdown<D: DropdownDelegate + 'static> {
    id: ElementId,
    focus_handle: FocusHandle,
    list: Entity<List<DropdownListDelegate<D>>>,
    size: Size,
    icon: Option<IconName>,
    open: bool,
    placeholder: Option<SharedString>,
    title_prefix: Option<SharedString>,
    selected_value: Option<<D::Item as DropdownItem>::Value>,
    empty: Empty,
    width: Length,
    menu_width: Length,
    /// Store the bounds of the input
    bounds: Bounds<Pixels>,
    disabled: bool,
    #[allow(dead_code)]
    subscriptions: Vec<Subscription>,
}

pub struct SearchableVec<T> {
    items: Vec<T>,
    matched_items: Vec<T>,
}

impl<T: DropdownItem + Clone> SearchableVec<T> {
    pub fn new(items: impl Into<Vec<T>>) -> Self {
        let items = items.into();

        Self {
            items: items.clone(),
            matched_items: items,
        }
    }
}

impl<T: DropdownItem + Clone> DropdownDelegate for SearchableVec<T> {
    type Item = T;

    fn len(&self) -> usize {
        self.matched_items.len()
    }

    fn get(&self, ix: usize) -> Option<&Self::Item> {
        self.matched_items.get(ix)
    }

    fn position<V>(&self, value: &V) -> Option<usize>
    where
        Self::Item: DropdownItem<Value = V>,
        V: PartialEq,
    {
        for (ix, item) in self.matched_items.iter().enumerate() {
            if item.value() == value {
                return Some(ix);
            }
        }

        None
    }

    fn can_search(&self) -> bool {
        true
    }

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        _cx: &mut Context<Dropdown<Self>>,
    ) -> Task<()> {
        self.matched_items = self
            .items
            .iter()
            .filter(|item| item.title().to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect();

        Task::ready(())
    }
}

impl From<Vec<SharedString>> for SearchableVec<SharedString> {
    fn from(items: Vec<SharedString>) -> Self {
        Self {
            items: items.clone(),
            matched_items: items,
        }
    }
}

impl<D> Dropdown<D>
where
    D: DropdownDelegate + 'static,
{
    pub fn new(
        id: impl Into<ElementId>,
        delegate: D,
        selected_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let delegate = DropdownListDelegate {
            delegate,
            dropdown: cx.entity().downgrade(),
            selected_index,
        };

        let searchable = delegate.delegate.can_search();

        let list = cx.new(|cx| {
            let mut list = List::new(delegate, window, cx).max_h(rems(20.));
            if !searchable {
                list = list.no_query();
            }
            list
        });

        let subscriptions = vec![
            cx.on_blur(&list.focus_handle(cx), window, Self::on_blur),
            cx.on_blur(&focus_handle, window, Self::on_blur),
        ];

        let mut this = Self {
            id: id.into(),
            focus_handle,
            placeholder: None,
            list,
            size: Size::Medium,
            icon: None,
            selected_value: None,
            open: false,
            title_prefix: None,
            empty: None,
            width: Length::Auto,
            menu_width: Length::Auto,
            bounds: Bounds::default(),
            disabled: false,
            subscriptions,
        };
        this.set_selected_index(selected_index, window, cx);
        this
    }

    /// Set the width of the dropdown input, default: Length::Auto
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Set the width of the dropdown menu, default: Length::Auto
    pub fn menu_width(mut self, width: impl Into<Length>) -> Self {
        self.menu_width = width.into();
        self
    }

    /// Set the placeholder for display when dropdown value is empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the right icon for the dropdown input, instead of the default arrow icon.
    pub fn icon(mut self, icon: impl Into<IconName>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set title prefix for the dropdown.
    ///
    /// e.g.: Country: United States
    ///
    /// You should set the label is `Country: `
    pub fn title_prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.title_prefix = Some(prefix.into());
        self
    }

    /// Set the disable state for the dropdown.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn empty<E, F>(mut self, f: F) -> Self
    where
        E: IntoElement,
        F: Fn(&Window, &App) -> E + 'static,
    {
        self.empty = Some(Box::new(move |window, cx| f(window, cx).into_any_element()));
        self
    }

    pub fn set_selected_index(
        &mut self,
        selected_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list.update(cx, |list, cx| {
            list.set_selected_index(selected_index, window, cx);
        });
        self.update_selected_value(window, cx);
    }

    pub fn set_selected_value(
        &mut self,
        selected_value: &<D::Item as DropdownItem>::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        <<D as DropdownDelegate>::Item as DropdownItem>::Value: PartialEq,
    {
        let delegate = self.list.read(cx).delegate();
        let selected_index = delegate.delegate.position(selected_value);
        self.set_selected_index(selected_index, window, cx);
    }

    pub fn selected_index(&self, _window: &Window, cx: &App) -> Option<usize> {
        self.list.read(cx).selected_index()
    }

    fn update_selected_value(&mut self, window: &Window, cx: &App) {
        self.selected_value = self
            .selected_index(window, cx)
            .and_then(|ix| self.list.read(cx).delegate().delegate.get(ix))
            .map(|item| item.value().clone());
    }

    pub fn selected_value(&self) -> Option<&<D::Item as DropdownItem>::Value> {
        self.selected_value.as_ref()
    }

    pub fn focus(&self, window: &mut Window, _: &mut App) {
        self.focus_handle.focus(window);
    }

    fn on_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // When the dropdown and dropdown menu are both not focused, close the dropdown menu.
        if self.list.focus_handle(cx).is_focused(window) || self.focus_handle.is_focused(window) {
            return;
        }

        self.open = false;
        cx.notify();
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.list.focus_handle(cx).focus(window);
        window.dispatch_action(Box::new(list::SelectPrev), cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.open = true;
        }

        self.list.focus_handle(cx).focus(window);
        window.dispatch_action(Box::new(list::SelectNext), cx);
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        // Propagate the event to the parent view, for example to the Modal to support ENTER to confirm.
        cx.propagate();

        if !self.open {
            self.open = true;
            cx.notify();
        } else {
            self.list.focus_handle(cx).focus(window);
            window.dispatch_action(Box::new(list::Confirm), cx);
        }
    }

    fn toggle_menu(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();

        self.open = !self.open;
        if self.open {
            self.list.focus_handle(cx).focus(window);
        }
        cx.notify();
    }

    fn escape(&mut self, _: &Escape, _window: &mut Window, cx: &mut Context<Self>) {
        // Propagate the event to the parent view, for example to the Modal to support ESC to close.
        cx.propagate();

        self.open = false;
        cx.notify();
    }

    fn display_title(&self, window: &Window, cx: &App) -> impl IntoElement {
        let title = if let Some(selected_index) = &self.selected_index(window, cx) {
            let title = self
                .list
                .read(cx)
                .delegate()
                .delegate
                .get(*selected_index)
                .map(|item| item.title().to_string())
                .unwrap_or_default();

            h_flex()
                .when_some(self.title_prefix.clone(), |this, prefix| this.child(prefix))
                .child(title.clone())
        } else {
            div().text_color(cx.theme().text_accent).child(
                self.placeholder
                    .clone()
                    .unwrap_or_else(|| "Please select".into()),
            )
        };

        title.when(self.disabled, |this| {
            this.cursor_not_allowed().text_color(cx.theme().text_muted)
        })
    }
}

impl<D> Sizable for Dropdown<D>
where
    D: DropdownDelegate + 'static,
{
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl<D> EventEmitter<DropdownEvent<D>> for Dropdown<D> where D: DropdownDelegate + 'static {}
impl<D> EventEmitter<DismissEvent> for Dropdown<D> where D: DropdownDelegate + 'static {}
impl<D> Focusable for Dropdown<D>
where
    D: DropdownDelegate + 'static,
{
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.open {
            self.list.focus_handle(cx)
        } else {
            self.focus_handle.clone()
        }
    }
}

impl<D> Render for Dropdown<D>
where
    D: DropdownDelegate + 'static,
{
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let view = cx.entity().clone();
        let bounds = self.bounds;
        let allow_open = !(self.open || self.disabled);
        let outline_visible = self.open || is_focused && !self.disabled;

        // If the size has change, set size to self.list, to change the QueryInput size.
        if self.list.read(cx).size != self.size {
            self.list
                .update(cx, |this, cx| this.set_size(self.size, window, cx))
        }

        div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::escape))
            .size_full()
            .relative()
            .input_text_size(self.size)
            .child(
                div()
                    .id("dropdown-input")
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(cx.theme().background)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .shadow_sm()
                    .map(|this| {
                        if self.disabled {
                            this.cursor_not_allowed()
                        } else {
                            this.cursor_pointer()
                        }
                    })
                    .overflow_hidden()
                    .input_text_size(self.size)
                    .map(|this| match self.width {
                        Length::Definite(l) => this.flex_none().w(l),
                        Length::Auto => this.w_full(),
                    })
                    .when(outline_visible, |this| this.outline(window, cx))
                    .input_size(self.size)
                    .when(allow_open, |this| {
                        this.on_click(cx.listener(Self::toggle_menu))
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_1()
                            .child(
                                div()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(self.display_title(window, cx)),
                            )
                            .map(|this| {
                                let icon = match self.icon.clone() {
                                    Some(icon) => icon,
                                    None => {
                                        if self.open {
                                            IconName::CaretUp
                                        } else {
                                            IconName::CaretDown
                                        }
                                    }
                                };

                                this.child(
                                    Icon::new(icon)
                                        .xsmall()
                                        .text_color(match self.disabled {
                                            true => cx.theme().icon_muted,
                                            false => cx.theme().icon,
                                        })
                                        .when(self.disabled, |this| this.cursor_not_allowed()),
                                )
                            }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, cx| view.update(cx, |r, _| r.bounds = bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
            .when(self.open, |this| {
                this.child(
                    deferred(
                        anchored().snap_to_window_with_margin(px(8.)).child(
                            div()
                                .occlude()
                                .map(|this| match self.menu_width {
                                    Length::Auto => this.w(bounds.size.width),
                                    Length::Definite(w) => this.w(w),
                                })
                                .child(
                                    v_flex()
                                        .occlude()
                                        .mt_1p5()
                                        .bg(cx.theme().background)
                                        .border_1()
                                        .border_color(cx.theme().border_focused)
                                        .rounded(cx.theme().radius)
                                        .shadow_md()
                                        .on_mouse_down_out(|_, _, cx| {
                                            cx.dispatch_action(&Escape);
                                        })
                                        .child(self.list.clone()),
                                )
                                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                                    this.escape(&Escape, window, cx);
                                })),
                        ),
                    )
                    .with_priority(1),
                )
            })
    }
}
