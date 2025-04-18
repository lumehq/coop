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

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Welcome> {
    Welcome::new(window, cx)
}

pub struct Welcome {
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
}

impl Welcome {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            name: "Welcome".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for Welcome {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        "👋".into_any_element()
    }

    fn closable(&self, _cx: &App) -> bool {
        self.closable
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

impl EventEmitter<PanelEvent> for Welcome {}

impl Focusable for Welcome {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Welcome {
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
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::NINE))
                            .font_semibold()
                            .text_sm(),
                    ),
            )
    }
}
