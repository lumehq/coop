use gpui::{
    div, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use ui::{
    button::Button,
    dock_area::panel::{Panel, PanelEvent},
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
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
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

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closeable(&self, _cx: &App) -> bool {
        self.closeable
    }

    fn zoomable(&self, _cx: &App) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }
}

impl EventEmitter<PanelEvent> for WelcomePanel {}

impl Focusable for WelcomePanel {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WelcomePanel {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
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
