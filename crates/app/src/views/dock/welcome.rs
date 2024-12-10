use coop_ui::{
    button::Button,
    dock::{DockItemState, Panel, PanelEvent, TitleStyle},
    popup_menu::PopupMenu,
    theme::{ActiveTheme, Colorize},
    StyledExt,
};
use gpui::*;

pub struct WelcomePanel {
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
}

impl WelcomePanel {
    pub fn new(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::view)
    }

    fn view(cx: &mut ViewContext<Self>) -> Self {
        Self {
            name: "Welcome".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for WelcomePanel {
    fn panel_name(&self) -> &'static str {
        "WelcomePanel"
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn title_style(&self, _cx: &WindowContext) -> Option<TitleStyle> {
        None
    }

    fn closeable(&self, _cx: &WindowContext) -> bool {
        self.closeable
    }

    fn zoomable(&self, _cx: &WindowContext) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &WindowContext) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _cx: &WindowContext) -> Vec<Button> {
        vec![]
    }

    fn dump(&self, _cx: &AppContext) -> DockItemState {
        DockItemState::new(self)
    }
}

impl EventEmitter<PanelEvent> for WelcomePanel {}

impl FocusableView for WelcomePanel {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WelcomePanel {
    fn render(&mut self, cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("coop on nostr.")
            .text_color(cx.theme().muted.darken(0.1))
            .font_black()
            .text_sm()
    }
}
