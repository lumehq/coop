use anyhow::anyhow;
use common::profile::RenderProfile;
use global::constants::ACCOUNT_D;
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::checkbox::Checkbox;
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::popup_menu::PopupMenu;
use ui::{Disableable, Icon, IconName, Sizable, StyledExt};

use crate::chatspace;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

pub struct Onboarding {
    name: SharedString,
    local_account: Entity<Option<Profile>>,
    loading: bool,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let local_account = cx.new(|_| None);

        let task = cx.background_spawn(async move {
            let database = shared_state().client().database();

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_D)
                .limit(1);

            if let Some(event) = database.query(filter).await?.first_owned() {
                let public_key = event
                    .tags
                    .public_keys()
                    .copied()
                    .collect_vec()
                    .first()
                    .cloned()
                    .unwrap();
                let metadata = database.metadata(public_key).await?.unwrap_or_default();
                let profile = Profile::new(public_key, metadata);

                Ok(profile)
            } else {
                Err(anyhow!("Not found"))
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(profile) = task.await {
                this.update(cx, |this, cx| {
                    this.local_account.update(cx, |this, cx| {
                        *this = Some(profile);
                        cx.notify();
                    });
                })
                .ok();
            }
        })
        .detach();

        Self {
            local_account,
            name: "Onboarding".into(),
            loading: false,
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.loading = status;
        cx.notify();
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
}

impl EventEmitter<PanelEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        const TITLE: &str = "Welcome to Coop!";
        const SUBTITLE: &str = "Secure Communication on Nostr.";

        let auto_login = AppSettings::get_global(cx).settings.auto_login;
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;

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
                                    .child(TITLE),
                            )
                            .child(div().text_color(cx.theme().text_muted).child(SUBTITLE)),
                    ),
            )
            .map(|this| {
                if let Some(profile) = self.local_account.read(cx).as_ref() {
                    this.relative()
                        .child(
                            div()
                                .id("account")
                                .mb_3()
                                .h_10()
                                .w_72()
                                .bg(cx.theme().element_background)
                                .text_color(cx.theme().element_foreground)
                                .rounded_lg()
                                .text_sm()
                                .map(|this| {
                                    if self.loading {
                                        this.child(
                                            div()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(Indicator::new().small()),
                                        )
                                    } else {
                                        this.child(
                                            div()
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .gap_2()
                                                .child("Continue as")
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_1()
                                                        .font_semibold()
                                                        .child(
                                                            Avatar::new(
                                                                profile.render_avatar(proxy),
                                                            )
                                                            .size(rems(1.5)),
                                                        )
                                                        .child(
                                                            div()
                                                                .pb_px()
                                                                .child(profile.render_name()),
                                                        ),
                                                ),
                                        )
                                    }
                                })
                                .hover(|this| this.bg(cx.theme().element_hover))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_loading(true, cx);
                                    Identity::global(cx).update(cx, |this, cx| {
                                        this.load(window, cx);
                                    });
                                })),
                        )
                        .child(
                            Checkbox::new("auto_login")
                                .label("Automatically log in next time")
                                .checked(auto_login)
                                .on_click(|_, _window, cx| {
                                    AppSettings::global(cx).update(cx, |this, cx| {
                                        this.settings.auto_login = !this.settings.auto_login;
                                        cx.notify();
                                    })
                                }),
                        )
                        .child(
                            div().w_24().absolute().bottom_4().right_4().child(
                                Button::new("unload")
                                    .icon(IconName::Logout)
                                    .label("Logout")
                                    .ghost()
                                    .small()
                                    .disabled(self.loading)
                                    .on_click(|_, window, cx| {
                                        Identity::global(cx).update(cx, |this, cx| {
                                            this.unload(window, cx);
                                        });
                                    }),
                            ),
                        )
                } else {
                    this.child(
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
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        chatspace::new_account(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("login_btn")
                                    .label("Already have an account? Log in.")
                                    .ghost()
                                    .underline()
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        chatspace::login(window, cx);
                                    })),
                            ),
                    )
                }
            })
    }
}
