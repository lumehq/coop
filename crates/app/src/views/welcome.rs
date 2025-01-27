use gpui::{
    div, svg, AnyElement, AppContext, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, VisualContext,
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
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(Self::view)
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
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

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closeable(&self, _window: &Window, _cx: &App) -> bool {
        self.closeable
    }

    fn zoomable(&self, _window: &Window, _cx: &App) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _window: &Window, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState::new(self)
    }
}

impl EventEmitter<PanelEvent> for WelcomePanel {}

impl Focusable for WelcomePanel {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WelcomePanel {
    fn render(&mut self, window: &mut gpui::Window, &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        svg()
                            .path("brand/coop.svg")
                            .size_12()
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                    )
                    .child(
                        div()
                            .child("coop on nostr.")
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::FOUR))
                            .font_black()
                            .text_sm(),
                    ),
            )
    }
}
