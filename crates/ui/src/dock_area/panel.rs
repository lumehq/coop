use super::DockArea;
use crate::{
    button::Button,
    dock_area::state::{PanelInfo, PanelState},
    popup_menu::PopupMenu,
};
use gpui::{
    AnyElement, AnyView, AppContext, EventEmitter, FocusHandle, FocusableView, Global, Hsla,
    IntoElement, SharedString, View, WeakView, WindowContext,
};
use nostr_sdk::prelude::Metadata;
use std::{collections::HashMap, sync::Arc};

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

pub trait Panel: EventEmitter<PanelEvent> + FocusableView {
    /// The name of the panel used to serialize, deserialize and identify the panel.
    ///
    /// This is used to identify the panel when deserializing the panel.
    /// Once you have defined a panel id, this must not be changed.
    fn panel_id(&self) -> SharedString;

    /// The optional metadata of the panel
    fn panel_metadata(&self) -> Option<Metadata> {
        None
    }

    /// The title of the panel
    fn title(&self, _cx: &WindowContext) -> AnyElement {
        SharedString::from("Untitled").into_any_element()
    }

    /// Whether the panel can be closed, default is `true`.
    fn closeable(&self, _cx: &WindowContext) -> bool {
        true
    }

    /// Return true if the panel is zoomable, default is `false`.
    fn zoomable(&self, _cx: &WindowContext) -> bool {
        true
    }

    /// The addition popup menu of the panel, default is `None`.
    fn popup_menu(&self, this: PopupMenu, _cx: &WindowContext) -> PopupMenu {
        this
    }

    /// The addition toolbar buttons of the panel used to show in the right of the title bar, default is `None`.
    fn toolbar_buttons(&self, _cx: &WindowContext) -> Vec<Button> {
        vec![]
    }

    /// Dump the panel, used to serialize the panel.
    fn dump(&self, _cx: &AppContext) -> PanelState {
        PanelState::new(self)
    }
}

pub trait PanelView: 'static + Send + Sync {
    fn panel_id(&self, cx: &WindowContext) -> SharedString;
    fn panel_metadata(&self, cx: &WindowContext) -> Option<Metadata>;
    fn title(&self, _cx: &WindowContext) -> AnyElement;
    fn closeable(&self, cx: &WindowContext) -> bool;
    fn zoomable(&self, cx: &WindowContext) -> bool;
    fn popup_menu(&self, menu: PopupMenu, cx: &WindowContext) -> PopupMenu;
    fn toolbar_buttons(&self, cx: &WindowContext) -> Vec<Button>;
    fn view(&self) -> AnyView;
    fn focus_handle(&self, cx: &AppContext) -> FocusHandle;
    fn dump(&self, cx: &AppContext) -> PanelState;
}

impl<T: Panel> PanelView for View<T> {
    fn panel_id(&self, cx: &WindowContext) -> SharedString {
        self.read(cx).panel_id()
    }

    fn panel_metadata(&self, cx: &WindowContext) -> Option<Metadata> {
        self.read(cx).panel_metadata()
    }

    fn title(&self, cx: &WindowContext) -> AnyElement {
        self.read(cx).title(cx)
    }

    fn closeable(&self, cx: &WindowContext) -> bool {
        self.read(cx).closeable(cx)
    }

    fn zoomable(&self, cx: &WindowContext) -> bool {
        self.read(cx).zoomable(cx)
    }

    fn popup_menu(&self, menu: PopupMenu, cx: &WindowContext) -> PopupMenu {
        self.read(cx).popup_menu(menu, cx)
    }

    fn toolbar_buttons(&self, cx: &WindowContext) -> Vec<Button> {
        self.read(cx).toolbar_buttons(cx)
    }

    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn focus_handle(&self, cx: &AppContext) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn dump(&self, cx: &AppContext) -> PanelState {
        self.read(cx).dump(cx)
    }
}

impl From<&dyn PanelView> for AnyView {
    fn from(handle: &dyn PanelView) -> Self {
        handle.view()
    }
}

impl<T: Panel> From<&dyn PanelView> for View<T> {
    fn from(value: &dyn PanelView) -> Self {
        value.view().downcast::<T>().unwrap()
    }
}

impl PartialEq for dyn PanelView {
    fn eq(&self, other: &Self) -> bool {
        self.view() == other.view()
    }
}

type Items = HashMap<
    String,
    Arc<
        dyn Fn(
            WeakView<DockArea>,
            &PanelState,
            &PanelInfo,
            &mut WindowContext,
        ) -> Box<dyn PanelView>,
    >,
>;

pub struct PanelRegistry {
    pub(super) items: Items,
}

impl PanelRegistry {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }
}

impl Default for PanelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Global for PanelRegistry {}

/// Register the Panel init by panel_name to global registry.
pub fn register_panel<F>(cx: &mut AppContext, panel_id: &str, deserialize: F)
where
    F: Fn(WeakView<DockArea>, &PanelState, &PanelInfo, &mut WindowContext) -> Box<dyn PanelView>
        + 'static,
{
    if cx.try_global::<PanelRegistry>().is_none() {
        cx.set_global(PanelRegistry::new());
    }

    cx.global_mut::<PanelRegistry>()
        .items
        .insert(panel_id.to_string(), Arc::new(deserialize));
}
