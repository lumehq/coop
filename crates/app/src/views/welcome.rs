use gpui::{
    div, AnyElement, AppContext, EventEmitter, FocusHandle, FocusableView, IntoElement,
    ParentElement, Render, SharedString, Styled, View, ViewContext, VisualContext, WindowContext,
};
use ui::{
    button::Button,
    dock_area::{
        panel::{Panel, PanelEvent},
        state::PanelState,
    },
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    StyledExt,
};

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
    fn panel_id(&self) -> SharedString {
        "WelcomePanel".into()
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
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

    fn dump(&self, _cx: &AppContext) -> PanelState {
        PanelState::new(self)
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
            .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE))
            .font_black()
            .text_sm()
    }
}
