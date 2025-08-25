use std::sync::Arc;

use anyhow::anyhow;
use auto_update::AutoUpdater;
use client_keys::ClientKeys;
use common::display::DisplayProfile;
use global::constants::{ACCOUNT_IDENTIFIER, DEFAULT_SIDEBAR_WIDTH};
use global::{global_channel, nostr_client, NostrSignal};
use gpui::prelude::FluentBuilder;
use gpui::{
    actions, div, px, rems, Action, App, AppContext, AsyncWindowContext, Axis, Context, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, WeakEntity, Window,
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
use ui::indicator::Indicator;
use ui::modal::ModalButtonProps;
use ui::popup_menu::PopupMenuExt;
use ui::tooltip::Tooltip;
use ui::{h_flex, v_flex, ContextModal, IconName, Root, Sizable, StyledExt};

use crate::views::compose::compose_button;
use crate::views::screening::Screening;
use crate::views::user_profile::UserProfile;
use crate::views::{
    account, chat, login, messaging_relays, new_account, onboarding, preferences, sidebar,
    user_profile, welcome,
};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<ChatSpace> {
    cx.new(|cx| ChatSpace::new(window, cx))
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
    _subscriptions: SmallVec<[Subscription; 4]>,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = Registry::global(cx);
        let client_keys = ClientKeys::global(cx);

        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| DockArea::new(window, cx));

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the client keys and show an alert modal if they fail to initialize
            cx.observe_in(&client_keys, window, |this, state, window, cx| {
                if !state.read(cx).has_keys() {
                    this.render_client_keys_modal(window, cx);
                } else {
                    this.load_local_account(window, cx);
                }
            }),
        );

        subscriptions.push(
            // Automatically run load function when UserProfile is created
            cx.observe_new::<UserProfile>(|this, window, cx| {
                if let Some(window) = window {
                    this.load(window, cx);
                }
            }),
        );

        subscriptions.push(
            // Automatically run load function when Screening is created
            cx.observe_new::<Screening>(|this, window, cx| {
                if let Some(window) = window {
                    this.load(window, cx);
                }
            }),
        );

        subscriptions.push(
            // Subscribe to open chat room requests
            cx.subscribe_in(
                &registry,
                window,
                |this, _e, event, window, cx| match event {
                    RegistrySignal::Open(room) => {
                        if let Some(room) = room.upgrade() {
                            this.dock.update(cx, |this, cx| {
                                let panel = chat::init(room, window, cx);
                                this.add_panel(Arc::new(panel), DockPlacement::Center, window, cx);
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
                },
            ),
        );

        tasks.push(
            // Continuously handle signals from the Nostr channel
            cx.spawn_in(window, async move |this, cx| {
                ChatSpace::handle_signal(this, cx).await
            }),
        );

        Self {
            dock,
            title_bar,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn handle_signal(e: WeakEntity<ChatSpace>, cx: &mut AsyncWindowContext) {
        let channel = global_channel();
        let mut is_open_proxy_modal = false;

        while let Ok(signal) = channel.1.recv().await {
            cx.update(|window, cx| {
                let registry = Registry::global(cx);

                match signal {
                    NostrSignal::SignerSet(public_key) => {
                        window.close_modal(cx);

                        // Setup the default layout for current workspace
                        e.update(cx, |this, cx| {
                            this.set_default_layout(window, cx);
                        })
                        .ok();

                        // Initialize identity
                        identity::init(public_key, window, cx);

                        // Load all chat rooms
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                        });
                    }
                    NostrSignal::SignerUnset => {
                        e.update(cx, |this, cx| {
                            this.set_onboarding_layout(window, cx);
                        })
                        .ok();
                    }
                    NostrSignal::ProxyDown => {
                        if !is_open_proxy_modal {
                            e.update(cx, |this, cx| {
                                this.render_proxy_modal(window, cx);
                            })
                            .ok();
                            is_open_proxy_modal = true;
                        }
                    }
                    // Load chat rooms and stop the loading status
                    NostrSignal::Finish => {
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                            this.set_loading(false, cx);
                            // Send a signal to refresh all opened rooms' messages
                            if let Some(ids) = ChatSpace::all_panels(window, cx) {
                                this.refresh_rooms(ids, cx);
                            }
                        });
                    }
                    // Load chat rooms without setting as finished
                    NostrSignal::PartialFinish => {
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                            // Send a signal to refresh all opened rooms' messages
                            if let Some(ids) = ChatSpace::all_panels(window, cx) {
                                this.refresh_rooms(ids, cx);
                            }
                        });
                    }
                    // Add the new metadata to the registry or update the existing one
                    NostrSignal::Metadata(event) => {
                        registry.update(cx, |this, cx| {
                            this.insert_or_update_person(event, cx);
                        });
                    }
                    // Convert the gift wrapped message to a message
                    NostrSignal::GiftWrap(event) => {
                        let identity = Identity::read_global(cx).public_key();
                        registry.update(cx, |this, cx| {
                            this.event_to_message(identity, event, window, cx);
                        });
                    }
                    NostrSignal::DmRelaysFound => {
                        //
                    }
                    NostrSignal::Notice(_msg) => {
                        // window.push_notification(msg, cx);
                    }
                };
            })
            .ok();
        }
    }

    pub fn set_onboarding_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    pub fn set_account_layout(
        &mut self,
        secret: String,
        profile: Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = Arc::new(account::init(secret, profile, window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn set_default_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak_dock = self.dock.downgrade();

        let sidebar = Arc::new(sidebar::init(window, cx));
        let center = Arc::new(welcome::init(window, cx));

        let left = DockItem::panel(sidebar);
        let center = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(vec![center], None, &weak_dock, window, cx)],
            vec![None],
            &weak_dock,
            window,
            cx,
        );

        self.dock.update(cx, |this, cx| {
            this.set_left_dock(left, Some(px(DEFAULT_SIDEBAR_WIDTH)), true, window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn load_local_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_IDENTIFIER)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                let metadata = client
                    .database()
                    .metadata(event.pubkey)
                    .await?
                    .unwrap_or_default();

                Ok((event.content, Profile::new(event.pubkey, metadata)))
            } else {
                Err(anyhow!("Empty"))
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok((secret, profile)) = task.await {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_account_layout(secret, profile, window, cx);
                    })
                    .ok();
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_onboarding_layout(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn on_settings(&mut self, _ev: &Settings, window: &mut Window, cx: &mut Context<Self>) {
        let view = preferences::init(window, cx);

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title(shared_t!("common.preferences"))
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

    fn on_sign_out(&mut self, _e: &Logout, _window: &mut Window, cx: &mut Context<Self>) {
        Identity::remove_global(cx);
        Registry::global(cx).update(cx, |this, cx| {
            this.reset(cx);
        });

        cx.background_spawn(async move {
            let client = nostr_client();
            let channel = global_channel();

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_IDENTIFIER);

            // Delete account
            client.database().delete(filter).await.ok();

            // Reset the nostr client
            client.reset().await;

            // Notify the channel about the signer being unset
            channel.0.send(NostrSignal::SignerUnset).await.ok();
        })
        .detach();
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

    fn render_proxy_modal(&mut self, window: &mut Window, cx: &mut App) {
        window.open_modal(cx, |this, _window, _cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .alert()
                .button_props(ModalButtonProps::default().ok_text(t!("common.open_browser")))
                .title(shared_t!("proxy.label"))
                .child(
                    v_flex()
                        .p_3()
                        .gap_1()
                        .w_full()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_sm()
                        .child(shared_t!("proxy.description")),
                )
                .on_ok(move |_e, _window, cx| {
                    cx.open_url("http://localhost:7400");
                    false
                })
        });
    }

    fn render_client_keys_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                                .child(shared_t!("startup.client_keys_warning")),
                        )
                        .child(shared_t!("startup.client_keys_desc")),
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

    fn render_titlebar_left_side(
        &mut self,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let registry = Registry::read_global(cx);
        let loading = registry.loading;

        h_flex()
            .gap_2()
            .child(compose_button())
            .when(loading, |this| {
                this.child(
                    h_flex()
                        .id("downloading")
                        .px_4()
                        .h_6()
                        .gap_1()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().elevated_surface_background)
                        .child(shared_t!("loading.label"))
                        .child(Indicator::new().xsmall())
                        .tooltip(|window, cx| {
                            Tooltip::new(t!("loading.tooltip"), window, cx).into()
                        }),
                )
            })
    }

    fn render_titlebar_right_side(
        &mut self,
        profile: &Profile,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let nip17_relays = Identity::read_global(cx).nip17_relays();

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
                            cx.restart();
                        }),
                )
            })
            .when_some(nip17_relays, |this, status| {
                this.when(!status, |this| this.child(messaging_relays::relay_button()))
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
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let logged_in = Identity::has_global(cx);

        // Only render titlebar child elements if user is logged in
        if logged_in {
            let identity = Identity::read_global(cx).public_key();
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
