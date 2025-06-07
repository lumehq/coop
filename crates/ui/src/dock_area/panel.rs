use gpui::{
    AnyElement, AnyView, App, Element, Entity, EventEmitter, FocusHandle, Focusable, Hsla, Render,
    SharedString, Window,
};

use crate::button::Button;
use crate::popup_menu::PopupMenu;

pub enum PanelEvent {
    ZoomIn,
    ZoomOut,
    LayoutChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelStyle {
    /// Display the TabBar when there are multiple tabs, otherwise display the simple title.
    Default,
    /// Always display the tab bar.
    TabBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleStyle {
    pub background: Hsla,
    pub foreground: Hsla,
}

pub trait Panel: EventEmitter<PanelEvent> + Render + Focusable {
    /// The name of the panel used to serialize, deserialize and identify the panel.
    ///
    /// This is used to identify the panel when deserializing the panel.
    /// Once you have defined a panel id, this must not be changed.
    fn panel_id(&self) -> SharedString;

    /// The title of the panel
    fn title(&self, _cx: &App) -> AnyElement {
        SharedString::from("Unnamed").into_any()
    }

    /// Whether the panel can be closed, default is `true`.
    fn closable(&self, _cx: &App) -> bool {
        true
    }

    /// Return true if the panel is zoomable, default is `false`.
    fn zoomable(&self, _cx: &App) -> bool {
        true
    }

    /// Return false to hide panel, true to show panel, default is `true`.
    ///
    /// This method called in Panel render, we should make sure it is fast.
    fn visible(&self, _cx: &App) -> bool {
        true
    }

    /// Set active state of the panel.
    ///
    /// This method will be called when the panel is active or inactive.
    ///
    /// The last_active_panel and current_active_panel will be touched when the panel is active.
    fn set_active(&self, _active: bool, _cx: &mut App) {}

    /// Set zoomed state of the panel.
    ///
    /// This method will be called when the panel is zoomed or unzoomed.
    ///
    /// Only current Panel will touch this method.
    fn set_zoomed(&self, _zoomed: bool, _cx: &mut App) {}

    /// The addition popup menu of the panel, default is `None`.
    fn popup_menu(&self, this: PopupMenu, _cx: &App) -> PopupMenu {
        this
    }

    /// The addition toolbar buttons of the panel used to show in the right of the title bar, default is `None`.
    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }
}

pub trait PanelView: 'static + Send + Sync {
    fn panel_id(&self, cx: &App) -> SharedString;
    fn title(&self, cx: &App) -> AnyElement;
    fn closable(&self, cx: &App) -> bool;
    fn zoomable(&self, cx: &App) -> bool;
    fn visible(&self, cx: &App) -> bool;
    fn set_active(&self, active: bool, cx: &mut App);
    fn set_zoomed(&self, zoomed: bool, cx: &mut App);
    fn popup_menu(&self, menu: PopupMenu, cx: &App) -> PopupMenu;
    fn toolbar_buttons(&self, window: &Window, cx: &App) -> Vec<Button>;
    fn view(&self) -> AnyView;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
}

impl<T: Panel> PanelView for Entity<T> {
    fn panel_id(&self, cx: &App) -> SharedString {
        self.read(cx).panel_id()
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.read(cx).title(cx)
    }

    fn closable(&self, cx: &App) -> bool {
        self.read(cx).closable(cx)
    }

    fn zoomable(&self, cx: &App) -> bool {
        self.read(cx).zoomable(cx)
    }

    fn visible(&self, cx: &App) -> bool {
        self.read(cx).visible(cx)
    }

    fn set_active(&self, active: bool, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.set_active(active, cx);
        })
    }

    fn set_zoomed(&self, zoomed: bool, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.set_zoomed(zoomed, cx);
        })
    }

    fn popup_menu(&self, menu: PopupMenu, cx: &App) -> PopupMenu {
        self.read(cx).popup_menu(menu, cx)
    }

    fn toolbar_buttons(&self, window: &Window, cx: &App) -> Vec<Button> {
        self.read(cx).toolbar_buttons(window, cx)
    }

    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }
}

impl From<&dyn PanelView> for AnyView {
    fn from(handle: &dyn PanelView) -> Self {
        handle.view()
    }
}

impl<T: Panel> From<&dyn PanelView> for Entity<T> {
    fn from(value: &dyn PanelView) -> Self {
        value.view().downcast::<T>().unwrap()
    }
}

impl PartialEq for dyn PanelView {
    fn eq(&self, other: &Self) -> bool {
        self.view() == other.view()
    }
}
