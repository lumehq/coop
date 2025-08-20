use std::sync::Arc;
use std::time::Duration;

use client_keys::ClientKeys;
use global::constants::{APP_NAME, NOSTR_CONNECT_RELAY, NOSTR_CONNECT_TIMEOUT};
use gpui::{
    div, relative, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use i18n::t;
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::popup_menu::PopupMenu;
use ui::{Icon, IconName, StyledExt};

use crate::chatspace;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

pub struct Onboarding {
    name: SharedString,
    focus_handle: FocusHandle,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let relay = RelayUrl::parse(NOSTR_CONNECT_RELAY).unwrap();
        let app_keys = ClientKeys::read_global(cx).keys();
        let uri = NostrConnectURI::client(app_keys.public_key(), vec![relay], APP_NAME);

        let signer = cx.new(|_| {
            let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT);
            let signer = NostrConnect::new(uri, app_keys, timeout, None).unwrap();

            Arc::new(signer)
        });

        Self {
            name: "Onboarding".into(),
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for Onboarding {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
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
            .child(
                div()
                    .mb_10()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        svg()
                            .path("brand/coop.svg")
                            .size_16()
                            .text_color(cx.theme().elevated_surface_background),
                    )
                    .child(
                        div()
                            .text_center()
                            .child(
                                div()
                                    .text_xl()
                                    .font_semibold()
                                    .line_height(relative(1.3))
                                    .child(SharedString::new(t!("welcome.title"))),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().text_muted)
                                    .child(SharedString::new(t!("welcome.subtitle"))),
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
                            .label(SharedString::new(t!("onboarding.start_messaging")))
                            .primary()
                            .reverse()
                            .on_click(cx.listener(move |_, _, window, cx| {
                                chatspace::new_account(window, cx);
                            })),
                    )
                    .child(
                        Button::new("login_btn")
                            .label(SharedString::new(t!("onboarding.already_have_account")))
                            .ghost()
                            .underline()
                            .on_click(cx.listener(move |_, _, window, cx| {
                                chatspace::login(window, cx);
                            })),
                    ),
            )
    }
}
