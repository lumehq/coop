use gpui::{
    div, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use theme::ActiveTheme;
use ui::button::Button;
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::popup_menu::PopupMenu;
use ui::Sizable;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Startup> {
    Startup::new(window, cx)
}

pub struct Startup {
    name: SharedString,
    focus_handle: FocusHandle,
}

impl Startup {
    fn new(_window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            name: "Welcome".into(),
            focus_handle: cx.focus_handle(),
        })
    }
}

impl Panel for Startup {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        "Startup".into_any_element()
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }
}

impl EventEmitter<PanelEvent> for Startup {}

impl Focusable for Startup {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Startup {
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
                    .gap_6()
                    .child(
                        svg()
                            .path("brand/coop.svg")
                            .size_12()
                            .text_color(cx.theme().elevated_surface_background),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_center()
                            .text_color(cx.theme().text_muted)
                            .child("Connection in progress")
                            .child(Indicator::new().small()),
                    ),
            )
    }
}
