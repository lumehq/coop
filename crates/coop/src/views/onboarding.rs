use gpui::{
    div, relative, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use ui::{
    button::{Button, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    Icon, IconName, StyledExt,
};

use crate::chat_space::ChatSpace;

use super::{login, new_account};

const LOGO_URL: &str = "brand/coop.svg";
const TITLE: &str = "Welcome to Coop!";
const SUBTITLE: &str = "Secure Communication on Nostr.";

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

pub struct Onboarding {
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            name: "Onboarding".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }

    fn open_new_account(&self, window: &mut Window, cx: &mut Context<Self>) {
        let new_account = new_account::init(window, cx);
        ChatSpace::set_center_panel(new_account, window, cx);
    }

    fn open_login(&self, window: &mut Window, cx: &mut Context<Self>) {
        let login = login::init(window, cx);
        ChatSpace::set_center_panel(login, window, cx);
    }
}

impl Panel for Onboarding {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
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

impl EventEmitter<PanelEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .py_4()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_10()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        svg()
                            .path(LOGO_URL)
                            .size_16()
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                    )
                    .child(
                        div()
                            .text_center()
                            .child(
                                div()
                                    .text_xl()
                                    .font_semibold()
                                    .line_height(relative(1.3))
                                    .child(TITLE),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                                    .child(SUBTITLE),
                            ),
                    ),
            )
            .child(
                div()
                    .w_72()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        Button::new("continue_btn")
                            .icon(Icon::new(IconName::ArrowRight))
                            .label("Start Messaging")
                            .primary()
                            .reverse()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_new_account(window, cx);
                            })),
                    )
                    .child(
                        Button::new("login_btn")
                            .label("Already have an account? Log in.")
                            .ghost()
                            .underline()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_login(window, cx);
                            })),
                    ),
            )
    }
}
