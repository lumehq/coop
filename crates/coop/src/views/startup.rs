use gpui::prelude::FluentBuilder;
use gpui::{
    div, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use i18n::{shared_t, t};
use identity::Identity;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::popup_menu::PopupMenu;
use ui::{h_flex, v_flex, Sizable, StyledExt};

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
            name: "Startup".into(),
            focus_handle: cx.focus_handle(),
        })
    }
}

impl Panel for Startup {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
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
        let identity = Identity::global(cx);
        let logging_in = identity.read(cx).logging_in();

        h_flex()
            .relative()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .gap_6()
                    .child(
                        svg()
                            .path("brand/coop.svg")
                            .size_12()
                            .text_color(cx.theme().elevated_surface_background),
                    )
                    .child(
                        h_flex()
                            .w_24()
                            .justify_center()
                            .gap_2()
                            .when(logging_in, |this| {
                                this.child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().text)
                                        .child(shared_t!("startup.auto_login_in_progress")),
                                )
                            })
                            .child(Indicator::new().small()),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .bottom_3()
                    .right_3()
                    .w_auto()
                    .h_auto()
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().text_muted)
                                    .child(shared_t!("startup.stuck")),
                            )
                            .child(
                                Button::new("reset")
                                    .label(t!("startup.reset"))
                                    .small()
                                    .ghost()
                                    .on_click(move |_, window, cx| {
                                        identity.update(cx, |this, cx| {
                                            this.unload(window, cx);
                                        });
                                        cx.restart();
                                    }),
                            ),
                    ),
            )
    }
}
