use gpui::{
    div, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use theme::ActiveTheme;
use ui::button::Button;
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::popup_menu::PopupMenu;
use ui::{v_flex, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Welcome> {
    Welcome::new(window, cx)
}

pub struct Welcome {
    name: SharedString,
    version: SharedString,
    focus_handle: FocusHandle,
}

impl Welcome {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let version = SharedString::from(format!("Version: {}", env!("CARGO_PKG_VERSION")));

        Self {
            version,
            name: "Welcome".into(),
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for Welcome {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, cx: &App) -> AnyElement {
        div()
            .child(
                svg()
                    .path("brand/coop.svg")
                    .size_4()
                    .text_color(cx.theme().element_background),
            )
            .into_any_element()
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
                v_flex()
                    .gap_2()
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path("brand/coop.svg")
                            .size_12()
                            .text_color(cx.theme().elevated_surface_background),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .justify_center()
                            .text_center()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(cx.theme().text_muted)
                                    .child("coop on nostr"),
                            )
                            .child(
                                div()
                                    .id("version")
                                    .text_color(cx.theme().text_placeholder)
                                    .text_xs()
                                    .child(self.version.clone())
                                    .on_click(|_, _window, cx| {
                                        cx.open_url("https://github.com/lumehq/coop/releases");
                                    }),
                            ),
                    ),
            )
    }
}
