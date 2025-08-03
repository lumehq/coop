use std::sync::Arc;

use auto_update::AutoUpdater;
use client_keys::ClientKeys;
use common::display::DisplayProfile;
use global::constants::DEFAULT_SIDEBAR_WIDTH;
use gpui::prelude::FluentBuilder;
use gpui::{
    actions, div, px, rems, Action, App, AppContext, Axis, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use i18n::{shared_t, t};
use identity::Identity;
use itertools::Itertools;
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use registry::{Registry, RegistrySignal};
use serde::Deserialize;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::{ActiveTheme, Theme, ThemeMode};
use title_bar::TitleBar;
use ui::actions::OpenProfile;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{ClosePanel, DockArea, DockItem};
use ui::modal::ModalButtonProps;
use ui::popup_menu::PopupMenuExt;
use ui::{h_flex, ContextModal, IconName, Root, Sizable, StyledExt};

use crate::views::compose::compose_button;
use crate::views::screening::Screening;
use crate::views::user_profile::UserProfile;
use crate::views::{
    backup_keys, chat, login, messaging_relays, new_account, onboarding, preferences, sidebar,
    startup, user_profile, welcome,
};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<ChatSpace> {
    ChatSpace::new(window, cx)
}

pub fn login(window: &mut Window, cx: &mut App) {
    let panel = login::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

pub fn new_account(window: &mut Window, cx: &mut App) {
    let panel = new_account::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

actions!(user, [DarkMode, Settings, Logout]);

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub enum PanelKind {
    Room(u64),
    // More kind will be added here
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub enum ModalKind {
    Profile,
    Compose,
    Relay,
    Onboarding,
    SetupRelay,
}

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = story, no_json)]
pub struct SelectLocale(SharedString);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = modal, no_json)]
pub struct ToggleModal {
    pub modal: ModalKind,
}

pub struct ChatSpace {
    title_bar: Entity<TitleBar>,
    dock: Entity<DockArea>,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 5]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| {
            let panel = Arc::new(startup::init(window, cx));
            let center = DockItem::panel(panel);
            let mut dock = DockArea::new(window, cx);
            // Initialize the dock area with the center panel
            dock.set_center(center, window, cx);
            dock
        });

        cx.new(|cx| {
            let registry = Registry::global(cx);
            let client_keys = ClientKeys::global(cx);
            let identity = Identity::global(cx);
            let mut subscriptions = smallvec![];

            // Observe the client keys and show an alert modal if they fail to initialize
            subscriptions.push(cx.observe_in(
                &client_keys,
                window,
                |_this: &mut Self, state, window, cx| {
                    if !state.read(cx).has_keys() {
                        let title = SharedString::new(t!("startup.client_keys_warning"));
                        let desc = SharedString::new(t!("startup.client_keys_desc"));

                        window.open_modal(cx, move |this, _window, cx| {
                            this.overlay_closable(false)
                                .show_close(false)
                                .keyboard(false)
                                .confirm()
                                .button_props(
                                    ModalButtonProps::default()
                                        .cancel_text(t!("startup.create_new_keys"))
                                        .ok_text(t!("common.allow")),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .h_40()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .items_center()
                                        .justify_center()
                                        .text_center()
                                        .text_sm()
                                        .child(
                                            div()
                                                .font_semibold()
                                                .text_color(cx.theme().text_muted)
                                                .child(title.clone()),
                                        )
                                        .child(desc.clone()),
                                )
                                .on_cancel(|_, _window, cx| {
                                    ClientKeys::global(cx).update(cx, |this, cx| {
                                        this.new_keys(cx);
                                    });
                                    // true: Close modal
                                    true
                                })
                                .on_ok(|_, window, cx| {
                                    ClientKeys::global(cx).update(cx, |this, cx| {
                                        this.load(window, cx);
                                    });
                                    // true: Close modal
                                    true
                                })
                        });
                    }
                },
            ));

            // Observe the identity and show onboarding if it fails to initialize
            subscriptions.push(cx.observe_in(
                &identity,
                window,
                |this: &mut Self, state, window, cx| {
                    if !state.read(cx).has_signer() {
                        this.set_onboarding_panels(window, cx);
                    } else {
                        this.set_chat_panels(window, cx);
                    }
                },
            ));

            // Automatically run load function when UserProfile is created
            subscriptions.push(cx.observe_new::<UserProfile>(|this, window, cx| {
                if let Some(window) = window {
                    this.load(window, cx);
                }
            }));

            // Automatically run load function when Screening is created
            subscriptions.push(cx.observe_new::<Screening>(|this, window, cx| {
                if let Some(window) = window {
                    this.load(window, cx);
                }
            }));

            // Subscribe to open chat room requests
            subscriptions.push(cx.subscribe_in(
                &registry,
                window,
                |this: &mut Self, _state, event, window, cx| {
                    match event {
                        RegistrySignal::Open(room) => {
                            if let Some(room) = room.upgrade() {
                                this.dock.update(cx, |this, cx| {
                                    let panel = chat::init(room, window, cx);
                                    // Load messages on panel creation
                                    panel.update(cx, |this, cx| {
                                        this.load_messages(window, cx);
                                    });

                                    this.add_panel(panel, DockPlacement::Center, window, cx);
                                });
                            } else {
                                window.push_notification(t!("common.room_error"), cx);
                            }
                        }
                        RegistrySignal::Close(..) => {
                            this.dock.update(cx, |this, cx| {
                                this.focus_tab_panel(window, cx);

                                cx.defer_in(window, |_, window, cx| {
                                    window.dispatch_action(Box::new(ClosePanel), cx);
                                    window.close_all_modals(cx);
                                });
                            });
                        }
                        _ => {}
                    }
                },
            ));

            Self {
                dock,
                title_bar,
                subscriptions,
            }
        })
    }

    pub fn set_onboarding_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    pub fn set_chat_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let registry = Registry::global(cx);
        let weak_dock = self.dock.downgrade();

        // The left panel will render sidebar
        let left = DockItem::panel(Arc::new(sidebar::init(window, cx)));

        // The center panel will render chat rooms (as tabs)
        let center = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(
                vec![Arc::new(welcome::init(window, cx))],
                None,
                &weak_dock,
                window,
                cx,
            )],
            vec![None],
            &weak_dock,
            window,
            cx,
        );

        // Update dock
        self.dock.update(cx, |this, cx| {
            this.set_left_dock(left, Some(px(DEFAULT_SIDEBAR_WIDTH)), true, window, cx);
            this.set_center(center, window, cx);
        });

        // Load all chat rooms from the database
        registry.update(cx, |this, cx| {
            this.load_rooms(window, cx);
        });
    }

    fn on_settings(&mut self, _ev: &Settings, window: &mut Window, cx: &mut Context<Self>) {
        let view = preferences::init(window, cx);
        let title = SharedString::new(t!("common.preferences"));

        window.open_modal(cx, move |modal, _, _| {
            modal
                .title(title.clone())
                .width(px(480.))
                .child(view.clone())
        });
    }

    fn on_dark_mode(&mut self, _ev: &DarkMode, window: &mut Window, cx: &mut Context<Self>) {
        if cx.theme().mode.is_dark() {
            Theme::change(ThemeMode::Light, Some(window), cx);
        } else {
            Theme::change(ThemeMode::Dark, Some(window), cx);
        }
    }

    fn on_sign_out(&mut self, _ev: &Logout, window: &mut Window, cx: &mut Context<Self>) {
        let identity = Identity::global(cx);
        // TODO: save current session?
        identity.update(cx, |this, cx| {
            this.unload(window, cx);
        });
    }

    fn on_open_profile(&mut self, ev: &OpenProfile, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = ev.0;
        let profile = user_profile::init(public_key, window, cx);

        window.open_modal(cx, move |this, _window, _cx| {
            this.alert()
                .show_close(true)
                .overlay_closable(true)
                .child(profile.clone())
                .button_props(ModalButtonProps::default().ok_text(t!("profile.njump")))
                .on_ok(move |_, _window, cx| {
                    let Ok(bech32) = public_key.to_bech32();
                    cx.open_url(&format!("https://njump.me/{bech32}"));
                    false
                })
        });
    }

    fn render_titlebar_left_side(
        &mut self,
        _window: &mut Window,
        _cx: &Context<Self>,
    ) -> impl IntoElement {
        let compose_button = compose_button().into_any_element();

        h_flex().gap_1().child(compose_button)
    }

    fn render_titlebar_right_side(
        &mut self,
        profile: &Profile,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let need_backup = Identity::read_global(cx).need_backup();
        let relay_ready = Identity::read_global(cx).relay_ready();

        let updating = AutoUpdater::read_global(cx).status.is_updating();
        let updated = AutoUpdater::read_global(cx).status.is_updated();

        h_flex()
            .gap_1()
            .when(updating, |this| {
                this.child(
                    h_flex()
                        .h_6()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .bg(cx.theme().ghost_element_background_alt)
                        .child(shared_t!("auto_update.updating")),
                )
            })
            .when(updated, |this| {
                this.child(
                    h_flex()
                        .id("updated")
                        .h_6()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .bg(cx.theme().ghost_element_background_alt)
                        .hover(|this| this.bg(cx.theme().ghost_element_hover))
                        .active(|this| this.bg(cx.theme().ghost_element_active))
                        .child(shared_t!("auto_update.updated"))
                        .on_click(|_, _window, cx| {
                            cx.restart(None);
                        }),
                )
            })
            .when_some(relay_ready, |this, status| {
                this.when(!status, |this| this.child(messaging_relays::relay_button()))
            })
            .when_some(need_backup, |this, keys| {
                this.child(backup_keys::backup_button(keys.to_owned()))
            })
            .child(
                Button::new("user")
                    .small()
                    .reverse()
                    .transparent()
                    .icon(IconName::CaretDown)
                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(1.49)))
                    .popup_menu(|this, _window, _cx| {
                        this.menu(t!("user.dark_mode"), Box::new(DarkMode))
                            .menu(t!("user.settings"), Box::new(Settings))
                            .separator()
                            .menu(t!("user.sign_out"), Box::new(Logout))
                    }),
            )
    }

    pub(crate) fn set_center_panel<P>(panel: P, window: &mut Window, cx: &mut App)
    where
        P: PanelView,
    {
        if let Some(Some(root)) = window.root::<Root>() {
            if let Ok(chatspace) = root.read(cx).view().clone().downcast::<ChatSpace>() {
                let panel = Arc::new(panel);
                let center = DockItem::panel(panel);

                chatspace.update(cx, |this, cx| {
                    this.dock.update(cx, |this, cx| {
                        this.set_center(center, window, cx);
                    });
                });
            }
        }
    }

    pub(crate) fn all_panels(window: &mut Window, cx: &mut App) -> Option<Vec<u64>> {
        let Some(Some(root)) = window.root::<Root>() else {
            return None;
        };

        let Ok(chatspace) = root.read(cx).view().clone().downcast::<ChatSpace>() else {
            return None;
        };

        let ids = chatspace
            .read(cx)
            .dock
            .read(cx)
            .items
            .panel_ids(cx)
            .into_iter()
            .filter_map(|panel| panel.parse::<u64>().ok())
            .collect_vec();

        Some(ids)
    }
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        // Only render titlebar element if user is logged in
        if let Some(identity) = Identity::read_global(cx).public_key() {
            let profile = Registry::read_global(cx).get_person(&identity, cx);

            let left_side = self
                .render_titlebar_left_side(window, cx)
                .into_any_element();

            let right_side = self
                .render_titlebar_right_side(&profile, window, cx)
                .into_any_element();

            self.title_bar.update(cx, |this, _cx| {
                this.set_children(vec![left_side, right_side]);
            })
        }

        div()
            .on_action(cx.listener(Self::on_settings))
            .on_action(cx.listener(Self::on_dark_mode))
            .on_action(cx.listener(Self::on_sign_out))
            .on_action(cx.listener(Self::on_open_profile))
            .relative()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    // Title Bar
                    .child(self.title_bar.clone())
                    // Dock
                    .child(self.dock.clone()),
            )
            // Notifications
            .children(notification_layer)
            // Modals
            .children(modal_layer)
    }
}
