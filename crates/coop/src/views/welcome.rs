use gpui::{
    div, svg, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::{v_flex, ActiveTheme, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Welcome> {
    cx.new(|cx| Welcome::new(window, cx))
}

pub struct Welcome {
    version: SharedString,
    focus_handle: FocusHandle,
}

impl Welcome {
    fn new(_window: &mut Window, cx: &mut App) -> Self {
        let version = SharedString::from(format!("Version: {}", env!("CARGO_PKG_VERSION")));

        Self {
            version,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for Welcome {
    fn panel_name(&self) -> &'static str {
        "Welcome"
    }

    fn title(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            svg()
                .path("brand/coop.svg")
                .size_4()
                .text_color(cx.theme().secondary),
        )
    }
}

impl EventEmitter<PanelEvent> for Welcome {}

impl Focusable for Welcome {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Welcome {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .text_color(cx.theme().muted),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .justify_center()
                            .text_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from("coop on nostr")),
                            )
                            .child(
                                div()
                                    .id("version")
                                    .text_color(cx.theme().muted_foreground)
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
