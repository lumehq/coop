use std::sync::Arc;

use account::Account;
use anyhow::Error;
use auto_update::{AutoUpdateStatus, AutoUpdater};
use chat::{ChatEvent, ChatRegistry};
use chat_ui::{CopyPublicKey, OpenPublicKey};
use common::{EventUtils, RenderedProfile, BOOTSTRAP_RELAYS, DEFAULT_SIDEBAR_WIDTH};
use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, rems, App, AppContext, Axis, ClipboardItem, Context, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, Window,
};
use i18n::{shared_t, t};
use itertools::Itertools;
use key_store::{Credential, KeyItem, KeyStore};
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use person::PersonRegistry;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;
use theme::{ActiveTheme, Theme, ThemeMode};
use title_bar::TitleBar;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{ClosePanel, DockArea, DockItem};
use ui::modal::ModalButtonProps;
use ui::popup_menu::PopupMenuExt;
use ui::{h_flex, v_flex, ContextModal, IconName, Root, Sizable};

use crate::actions::{reset, DarkMode, Logout, ReloadMetadata, Settings};
use crate::views::compose::compose_button;
use crate::views::{
    login, new_account, onboarding, preferences, sidebar, startup, user_profile, welcome,
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

#[derive(Debug)]
pub struct ChatSpace {
    /// App's Title Bar
    title_bar: Entity<TitleBar>,

    /// App's Dock Area
    dock: Entity<DockArea>,

    /// All subscriptions for observing the app state
    _subscriptions: SmallVec<[Subscription; 3]>,

    /// All long running tasks
    _tasks: SmallVec<[Task<()>; 5]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat = ChatRegistry::global(cx);
        let keystore = KeyStore::global(cx);
        let account = Account::global(cx);

        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| DockArea::new(window, cx));

        let mut subscriptions = smallvec![];
        let tasks = smallvec![];

        subscriptions.push(
            // Automatically sync theme with system appearance
            window.observe_window_appearance(|window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
            }),
        );

        subscriptions.push(
            // Observe account entity changes
            cx.observe_in(&account, window, move |this, state, window, cx| {
                if state.read(cx).has_account() {
                    this.set_default_layout(window, cx);
                };
            }),
        );

        subscriptions.push(
            // Observe keystore entity changes
            cx.observe_in(&keystore, window, move |_this, state, window, cx| {
                if state.read(cx).initialized {
                    let backend = state.read(cx).backend();

                    cx.spawn_in(window, async move |this, cx| {
                        let result = backend
                            .read_credentials(&KeyItem::User.to_string(), cx)
                            .await;

                        this.update_in(cx, |this, window, cx| {
                            match result {
                                Ok(Some((user, secret))) => {
                                    let credential = Credential::new(user, secret);
                                    this.set_startup_layout(credential, window, cx);
                                }
                                _ => {
                                    this.set_onboarding_layout(window, cx);
                                }
                            };
                        })
                        .ok();
                    })
                    .detach();
                }
            }),
        );

        subscriptions.push(
            // Observe all events emitted by the chat registry
            cx.subscribe_in(&chat, window, move |this, chat, ev, window, cx| {
                match ev {
                    ChatEvent::OpenRoom(id) => {
                        if let Some(room) = chat.read(cx).room(id, cx) {
                            this.dock.update(cx, |this, cx| {
                                let panel = chat_ui::init(room, window, cx);
                                this.add_panel(Arc::new(panel), DockPlacement::Center, window, cx);
                            });
                        }
                    }
                    ChatEvent::CloseRoom(..) => {
                        this.dock.update(cx, |this, cx| {
                            this.focus_tab_panel(window, cx);

                            cx.defer_in(window, |_, window, cx| {
                                window.dispatch_action(Box::new(ClosePanel), cx);
                                window.close_all_modals(cx);
                            });
                        });
                    }
                    _ => {}
                };
            }),
        );

        Self {
            dock,
            title_bar,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    fn set_onboarding_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn set_startup_layout(&mut self, cre: Credential, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(startup::init(cre, window, cx));
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

    fn on_settings(&mut self, _ev: &Settings, window: &mut Window, cx: &mut Context<Self>) {
        let view = preferences::init(window, cx);

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title(shared_t!("common.preferences"))
                .width(px(580.))
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

    fn on_reload_metadata(
        &mut self,
        _ev: &ReloadMetadata,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
            let filter = Filter::new().kind(Kind::PrivateDirectMessage);

            let pubkeys: Vec<PublicKey> = client
                .database()
                .query(filter)
                .await?
                .into_iter()
                .flat_map(|event| event.all_pubkeys())
                .unique()
                .collect();

            let filter = Filter::new()
                .kind(Kind::Metadata)
                .limit(pubkeys.len())
                .authors(pubkeys);

            client
                .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                .await?;

            Ok(())
        });

        cx.spawn_in(window, async move |_, cx| {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    window.push_notification(t!("common.refreshed"), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn on_sign_out(&mut self, _e: &Logout, _window: &mut Window, cx: &mut Context<Self>) {
        reset(cx);
    }

    fn on_open_pubkey(&mut self, ev: &OpenPublicKey, window: &mut Window, cx: &mut Context<Self>) {
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

    fn on_copy_pubkey(&mut self, ev: &CopyPublicKey, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(bech32) = ev.0.to_bech32();
        cx.write_to_clipboard(ClipboardItem::new_string(bech32));
        window.push_notification(t!("common.copied"), cx);
    }

    #[allow(dead_code)]
    fn get_all_panel_ids(&self, cx: &App) -> Option<Vec<u64>> {
        let ids: Vec<u64> = self
            .dock
            .read(cx)
            .items
            .panel_ids(cx)
            .into_iter()
            .filter_map(|panel| panel.parse::<u64>().ok())
            .collect();

        Some(ids)
    }

    fn set_center_panel<P>(panel: P, window: &mut Window, cx: &mut App)
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

    fn render_keyring_warning(window: &mut Window, cx: &mut App) {
        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .alert()
                .button_props(ModalButtonProps::default().ok_text(t!("common.continue")))
                .title(shared_t!("keyring_disable.label"))
                .child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .child(shared_t!("keyring_disable.body_1"))
                        .child(shared_t!("keyring_disable.body_2"))
                        .child(shared_t!("keyring_disable.body_3"))
                        .child(shared_t!("keyring_disable.body_4"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().danger_foreground)
                                .child(shared_t!("keyring_disable.body_5")),
                        ),
                )
        });
    }

    fn titlebar_left(&mut self, _window: &mut Window, cx: &Context<Self>) -> impl IntoElement {
        let chat = ChatRegistry::global(cx);
        let status = chat.read(cx).loading;

        if !Account::has_global(cx) {
            return div();
        }

        h_flex()
            .gap_2()
            .h_6()
            .w_full()
            .child(compose_button())
            .when(status, |this| {
                this.child(deferred(
                    h_flex()
                        .px_2()
                        .h_6()
                        .gap_1()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().surface_background)
                        .child(shared_t!("loading.label")),
                ))
            })
    }

    fn titlebar_right(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let file_keystore = KeyStore::global(cx).read(cx).is_using_file_keystore();
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let auto_update = AutoUpdater::global(cx);

        h_flex()
            .gap_1()
            .map(|this| match auto_update.read(cx).status.as_ref() {
                AutoUpdateStatus::Checking => this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().text_muted)
                        .child(SharedString::from("Checking for Coop updates...")),
                ),
                AutoUpdateStatus::Installing => this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().text_muted)
                        .child(SharedString::from("Installing updates...")),
                ),
                AutoUpdateStatus::Errored { msg } => this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().text_muted)
                        .child(SharedString::from(msg.as_ref())),
                ),
                AutoUpdateStatus::Updated => this.child(
                    div()
                        .id("restart")
                        .text_xs()
                        .text_color(cx.theme().text_muted)
                        .child(SharedString::from("Updated. Click to restart"))
                        .on_click(|_ev, _window, cx| {
                            cx.restart();
                        }),
                ),
                _ => this.child(div()),
            })
            .when(file_keystore, |this| {
                this.child(
                    Button::new("keystore-warning")
                        .icon(IconName::Warning)
                        .label("Keyring Disabled")
                        .ghost()
                        .xsmall()
                        .rounded()
                        .on_click(move |_ev, window, cx| {
                            Self::render_keyring_warning(window, cx);
                        }),
                )
            })
            .when(Account::has_global(cx), |this| {
                let persons = PersonRegistry::global(cx);
                let account = Account::global(cx);
                let public_key = account.read(cx).public_key();
                let profile = persons.read(cx).get_person(&public_key, cx);

                this.child(
                    Button::new("user")
                        .small()
                        .reverse()
                        .transparent()
                        .icon(IconName::CaretDown)
                        .child(Avatar::new(profile.avatar(proxy)).size(rems(1.49)))
                        .popup_menu(|this, _window, _cx| {
                            this.menu(t!("user.dark_mode"), Box::new(DarkMode))
                                .menu(t!("user.settings"), Box::new(Settings))
                                .separator()
                                .menu(t!("user.reload_metadata"), Box::new(ReloadMetadata))
                                .separator()
                                .menu(t!("user.sign_out"), Box::new(Logout))
                        }),
                )
            })
    }
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        let left = self.titlebar_left(window, cx).into_any_element();
        let right = self.titlebar_right(window, cx).into_any_element();

        // Update title bar children
        self.title_bar.update(cx, |this, _cx| {
            this.set_children(vec![left, right]);
        });

        div()
            .id(SharedString::from("chatspace"))
            .on_action(cx.listener(Self::on_settings))
            .on_action(cx.listener(Self::on_dark_mode))
            .on_action(cx.listener(Self::on_sign_out))
            .on_action(cx.listener(Self::on_open_pubkey))
            .on_action(cx.listener(Self::on_copy_pubkey))
            .on_action(cx.listener(Self::on_reload_metadata))
            .relative()
            .size_full()
            .child(
                v_flex()
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
