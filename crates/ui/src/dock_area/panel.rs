use crate::{button::Button, popup_menu::PopupMenu};
use gpui::{
    AnyElement, AnyView, App, Entity, EventEmitter, FocusHandle, Focusable, Hsla, IntoElement,
    Render, SharedString, Window,
};

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

    /// The optional facepile of the panel
    fn panel_facepile(&self, _cx: &App) -> Option<Vec<String>> {
        None
    }

    /// The title of the panel
    fn title(&self, _cx: &App) -> AnyElement {
        SharedString::from("Unamed").into_any_element()
    }

    /// Whether the panel can be closed, default is `true`.
    fn closeable(&self, _cx: &App) -> bool {
        true
    }

    /// Return true if the panel is zoomable, default is `false`.
    fn zoomable(&self, _cx: &App) -> bool {
        true
    }

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
    fn panel_facepile(&self, cx: &App) -> Option<Vec<String>>;
    fn title(&self, cx: &App) -> AnyElement;
    fn closeable(&self, cx: &App) -> bool;
    fn zoomable(&self, cx: &App) -> bool;
    fn popup_menu(&self, menu: PopupMenu, cx: &App) -> PopupMenu;
    fn toolbar_buttons(&self, window: &Window, cx: &App) -> Vec<Button>;
    fn view(&self) -> AnyView;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
}

impl<T: Panel> PanelView for Entity<T> {
    fn panel_id(&self, cx: &App) -> SharedString {
        self.read(cx).panel_id()
    }

    fn panel_facepile(&self, cx: &App) -> Option<Vec<String>> {
        self.read(cx).panel_facepile(cx)
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.read(cx).title(cx)
    }

    fn closeable(&self, cx: &App) -> bool {
        self.read(cx).closeable(cx)
    }

    fn zoomable(&self, cx: &App) -> bool {
        self.read(cx).zoomable(cx)
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
