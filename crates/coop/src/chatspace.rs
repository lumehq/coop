use std::sync::Arc;

use account::Account;
use auto_update::{AutoUpdateStatus, AutoUpdater};
use chat::{ChatEvent, ChatRegistry};
use common::{RenderedProfile, DEFAULT_SIDEBAR_WIDTH};
use encryption::Encryption;
use encryption_ui::EncryptionPanel;
use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, App, AppContext, Axis, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
};
use gpui_component::avatar::Avatar;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::dialog::DialogButtonProps;
use gpui_component::dock::{DockArea, DockItem, DockPlacement, PanelView};
use gpui_component::menu::DropdownMenu;
use gpui_component::popover::Popover;
use gpui_component::{
    h_flex, v_flex, ActiveTheme, IconName, Root, Sizable, Theme, ThemeMode, TitleBar, WindowExt,
};
use i18n::{shared_t, t};
use key_store::{Credential, KeyItem, KeyStore};
use person::PersonRegistry;
use relay_auth::RelayAuth;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};

use crate::actions::{reset, DarkMode, KeyringPopup, Logout, Settings, ViewProfile, ViewRelays};
use crate::views::compose::compose_button;
use crate::views::{onboarding, preferences, setup_relay, startup, welcome};
use crate::{login, new_identity, sidebar, user};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<ChatSpace> {
    cx.new(|cx| ChatSpace::new(window, cx))
}

pub fn onboarding(window: &mut Window, cx: &mut App) {
    let panel = onboarding::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

pub fn login(window: &mut Window, cx: &mut App) {
    let panel = login::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

pub fn new_account(window: &mut Window, cx: &mut App) {
    let panel = new_identity::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

#[derive(Debug)]
pub struct ChatSpace {
    /// App's Dock Area
    dock: Entity<DockArea>,

    /// App's Encryption Panel
    encryption_panel: Entity<EncryptionPanel>,

    /// Determines if the chat space is ready to use
    ready: bool,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 4]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat = ChatRegistry::global(cx);
        let keystore = KeyStore::global(cx);
        let account = Account::global(cx);
        let encryption_panel = encryption_ui::init(window, cx);

        // App's dock area
        let dock = cx.new(|cx| {
            DockArea::new("dock", None, window, cx)
                .panel_style(gpui_component::dock::PanelStyle::TabBar)
        });

        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Automatically sync theme with system appearance
            window.observe_window_appearance(|window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
            }),
        );

        subscriptions.push(
            // Observe account entity changes
            cx.observe_in(&account, window, move |this, state, window, cx| {
                if !this.ready && state.read(cx).has_account() {
                    this.set_default_layout(window, cx);

                    // Load all chat room in the database if available
                    let chat = ChatRegistry::global(cx);
                    chat.update(cx, |this, cx| {
                        this.get_rooms(cx);
                    });
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
                                this.add_panel(
                                    Arc::new(panel),
                                    DockPlacement::Center,
                                    None,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                    ChatEvent::CloseRoom(..) => {
                        this.dock.update(cx, |_this, cx| {
                            //window.dispatch_action(Box::new(ClosePanel), cx);
                            window.close_all_dialogs(cx);
                        });
                    }
                    _ => {}
                };
            }),
        );

        subscriptions.push(
            // Observe the chat registry
            cx.observe(&chat, move |_this, chat, cx| {
                // let ids = this.get_all_panels(cx);
                // TODO: rewrite

                chat.update(cx, |this, cx| {
                    this.refresh_rooms(None, cx);
                });
            }),
        );

        Self {
            dock,
            encryption_panel,
            ready: false,
            _subscriptions: subscriptions,
        }
    }

    fn set_onboarding_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.set_center(center, window, cx);
        });
    }

    fn set_startup_layout(&mut self, cre: Credential, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(startup::init(cre, window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.set_center(center, window, cx);
        });
    }

    fn set_default_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sidebar = Arc::new(sidebar::init(window, cx));
        let center = Arc::new(welcome::init(window, cx));
        let weak_dock = self.dock.downgrade();

        self.dock.update(cx, |this, cx| {
            this.set_left_dock(
                DockItem::panel(sidebar),
                Some(px(DEFAULT_SIDEBAR_WIDTH)),
                true,
                window,
                cx,
            );
            this.set_center(
                DockItem::split_with_sizes(
                    Axis::Vertical,
                    vec![DockItem::tabs(vec![center], &weak_dock, window, cx)],
                    vec![None],
                    &weak_dock,
                    window,
                    cx,
                ),
                window,
                cx,
            );
        });
        self.ready = true;
    }

    fn on_settings(&mut self, _ev: &Settings, window: &mut Window, cx: &mut Context<Self>) {
        let view = preferences::init(window, cx);

        window.open_dialog(cx, move |modal, _window, _cx| {
            modal
                .title(shared_t!("common.preferences"))
                .width(px(520.))
                .child(view.clone())
        });
    }

    fn on_profile(&mut self, _ev: &ViewProfile, window: &mut Window, cx: &mut Context<Self>) {
        let view = user::init(window, cx);
        let entity = view.downgrade();

        window.open_dialog(cx, move |modal, _window, _cx| {
            let entity = entity.clone();

            modal
                .title("Profile")
                .confirm()
                .child(view.clone())
                .button_props(DialogButtonProps::default().ok_text("Update"))
                .on_ok(move |_, window, cx| {
                    entity
                        .update(cx, |this, cx| {
                            let persons = PersonRegistry::global(cx);
                            let set_metadata = this.set_metadata(cx);

                            cx.spawn_in(window, async move |this, cx| {
                                let result = set_metadata.await;

                                this.update_in(cx, |_, window, cx| {
                                    match result {
                                        Ok(profile) => {
                                            persons.update(cx, |this, cx| {
                                                this.insert_or_update_person(profile, cx);
                                                // Close the edit profile modal
                                                window.close_all_dialogs(cx);
                                            });
                                        }
                                        Err(e) => {
                                            window.push_notification(e.to_string(), cx);
                                        }
                                    };
                                })
                                .ok();
                            })
                            .detach();
                        })
                        .ok();

                    // false to keep the modal open
                    false
                })
        });
    }

    fn on_relays(&mut self, _ev: &ViewRelays, window: &mut Window, cx: &mut Context<Self>) {
        let view = setup_relay::init(window, cx);
        let entity = view.downgrade();

        window.open_dialog(cx, move |this, _window, _cx| {
            let entity = entity.clone();

            this.confirm()
                .title(shared_t!("relays.modal"))
                .child(view.clone())
                .button_props(DialogButtonProps::default().ok_text(t!("common.update")))
                .on_ok(move |_, window, cx| {
                    entity
                        .update(cx, |this, cx| {
                            this.set_relays(window, cx);
                        })
                        .ok();

                    // false to keep the modal open
                    false
                })
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
        reset(cx);
    }

    /*
    fn on_open_pubkey(&mut self, ev: &OpenPublicKey, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = ev.0;
        let view = viewer::init(public_key, window, cx);

        window.open_dialog(cx, move |this, _window, _cx| {
            this.alert()
                .close_button(true)
                .overlay_closable(true)
                .child(view.clone())
                .button_props(DialogButtonProps::default().ok_text("View on njump.me"))
                .on_ok(move |_, _window, cx| {
                    let bech32 = public_key.to_bech32().unwrap();
                    let url = format!("https://njump.me/{bech32}");

                    // Open the URL in the default browser
                    cx.open_url(&url);

                    // false to keep the modal open
                    false
                })
        });
    }

    fn on_copy_pubkey(&mut self, ev: &CopyPublicKey, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(bech32) = ev.0.to_bech32();
        cx.write_to_clipboard(ClipboardItem::new_string(bech32));
        window.push_notification(shared_t!("common.copied"), cx);
    }
    */

    fn on_keyring(&mut self, _ev: &KeyringPopup, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, move |this, _window, _cx| {
            this.close_button(true)
                .title(shared_t!("keyring_disable.label"))
                .child(
                    v_flex()
                        .gap_2()
                        .pb_4()
                        .text_sm()
                        .child(shared_t!("keyring_disable.body_1"))
                        .child(shared_t!("keyring_disable.body_2"))
                        .child(shared_t!("keyring_disable.body_3")),
                )
        });
    }

    fn titlebar_left(&mut self, _window: &mut Window, cx: &Context<Self>) -> impl IntoElement {
        let account = Account::global(cx);
        let chat = ChatRegistry::global(cx);
        let status = chat.read(cx).loading;

        if !account.read(cx).has_account() {
            return div();
        }

        h_flex()
            .gap_2()
            .h_6()
            .w_full()
            .child(compose_button(cx))
            .when(status, |this| {
                this.child(deferred(
                    h_flex()
                        .px_2()
                        .h_6()
                        .gap_1()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().muted)
                        .child(shared_t!("loading.label")),
                ))
            })
    }

    fn titlebar_right(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let auto_update = AutoUpdater::global(cx);
        let account = Account::global(cx);
        let relay_auth = RelayAuth::global(cx);
        let pending_requests = relay_auth.read(cx).pending_requests(cx);
        let encryption_panel = self.encryption_panel.downgrade();

        h_flex()
            .pr_2()
            .gap_2()
            .map(|this| match auto_update.read(cx).status.as_ref() {
                AutoUpdateStatus::Checking => this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from("Checking for Coop updates...")),
                ),
                AutoUpdateStatus::Installing => this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from("Installing updates...")),
                ),
                AutoUpdateStatus::Errored { msg } => this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(msg.as_ref())),
                ),
                AutoUpdateStatus::Updated => this.child(
                    div()
                        .id("restart")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from("Updated. Click to restart"))
                        .on_click(|_ev, _window, cx| {
                            cx.restart();
                        }),
                ),
                _ => this.child(div()),
            })
            .when(pending_requests > 0, |this| {
                this.child(
                    h_flex()
                        .id("requests")
                        .h_6()
                        .px_2()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().warning)
                        .text_color(cx.theme().warning_foreground)
                        .hover(|this| this.bg(cx.theme().warning_hover))
                        .active(|this| this.bg(cx.theme().warning_active))
                        .child(shared_t!("auth.requests", u = pending_requests))
                        .on_click(move |_ev, window, cx| {
                            relay_auth.update(cx, |this, cx| {
                                this.re_ask(window, cx);
                            });
                        }),
                )
            })
            .when(account.read(cx).has_account(), |this| {
                let account = Account::global(cx);
                let public_key = account.read(cx).public_key();

                let persons = PersonRegistry::global(cx);
                let profile = persons.read(cx).get_person(&public_key, cx);

                let encryption = Encryption::global(cx);
                let has_encryption = encryption.read(cx).has_encryption(cx);

                let keystore = KeyStore::global(cx);
                let is_using_file_keystore = keystore.read(cx).is_using_file_keystore();

                let keyring_label = if is_using_file_keystore {
                    SharedString::from("Disabled")
                } else {
                    SharedString::from("Enabled")
                };

                this.child(
                    h_flex()
                        .gap_1()
                        .child(
                            Popover::new("encryption")
                                .trigger(
                                    Button::new("encryption-trigger")
                                        .tooltip("Manage Encryption Key")
                                        .icon(IconName::Plus)
                                        .rounded(cx.theme().radius)
                                        .small()
                                        .map(|this| match has_encryption {
                                            true => this.ghost(),
                                            false => this.warning(),
                                        }),
                                )
                                .content(move |_this, _window, _cx| {
                                    let encryption_panel = encryption_panel.clone();

                                    if let Some(view) = encryption_panel.upgrade() {
                                        view.clone().into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }
                                }),
                        )
                        .child(
                            Button::new("user")
                                .small()
                                .text()
                                .child(Avatar::new().src(profile.avatar(proxy)).small())
                                .dropdown_caret(true)
                                .dropdown_menu(move |this, _window, _cx| {
                                    this.label(profile.display_name())
                                        .menu_with_icon(
                                            "Profile",
                                            IconName::ArrowUp,
                                            Box::new(ViewProfile),
                                        )
                                        .menu_with_icon(
                                            "Messaging Relays",
                                            IconName::ArrowUp,
                                            Box::new(ViewRelays),
                                        )
                                        .separator()
                                        .label(SharedString::from("Keyring Service"))
                                        .menu_with_icon_and_disabled(
                                            keyring_label.clone(),
                                            IconName::ArrowUp,
                                            Box::new(KeyringPopup),
                                            !is_using_file_keystore,
                                        )
                                        .separator()
                                        .menu_with_icon(
                                            "Dark Mode",
                                            IconName::Sun,
                                            Box::new(DarkMode),
                                        )
                                        .menu_with_icon(
                                            "Settings",
                                            IconName::Settings,
                                            Box::new(Settings),
                                        )
                                        .menu_with_icon(
                                            "Sign Out",
                                            IconName::ArrowUp,
                                            Box::new(Logout),
                                        )
                                }),
                        ),
                )
            })
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
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let current_panel = self.dock.read(cx).items().view();
        let panel_name = current_panel.panel_name(cx);

        div()
            .id(SharedString::from("chatspace"))
            .on_action(cx.listener(Self::on_settings))
            .on_action(cx.listener(Self::on_profile))
            .on_action(cx.listener(Self::on_relays))
            .on_action(cx.listener(Self::on_dark_mode))
            .on_action(cx.listener(Self::on_sign_out))
            .on_action(cx.listener(Self::on_keyring))
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    // Title Bar
                    .child(
                        TitleBar::new()
                            .when(self.ready, |this| {
                                this.child(self.titlebar_left(window, cx))
                                    .child(self.titlebar_right(window, cx))
                            })
                            .when(!self.ready, |this| {
                                this.bg(cx.theme().background)
                                    .border_color(gpui::transparent_black())
                            })
                            .when(panel_name == "Login", |this| {
                                let title = current_panel.title(window, cx);
                                this.child(title)
                            })
                            .when(panel_name == "NewAccount", |this| {
                                let title = current_panel.title(window, cx);
                                this.child(title)
                            }),
                    )
                    // Dock
                    .child(self.dock.clone()),
            )
            // Notifications
            .children(notification_layer)
            // Modals
            .children(modal_layer)
    }
}
