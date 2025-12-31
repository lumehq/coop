use std::sync::Arc;

use account::Account;
use auto_update::{AutoUpdateStatus, AutoUpdater};
use chat::{ChatEvent, ChatRegistry};
use chat_ui::{CopyPublicKey, OpenPublicKey};
use common::{RenderedProfile, DEFAULT_SIDEBAR_WIDTH};
use encryption::Encryption;
use encryption_ui::EncryptionPanel;
use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, relative, rems, App, AppContext, Axis, ClipboardItem, Context, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window,
};
use key_store::{Credential, KeyItem, KeyStore};
use nostr_connect::prelude::*;
use person::PersonRegistry;
use relay_auth::RelayAuth;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::{ActiveTheme, Theme, ThemeMode, ThemeRegistry};
use title_bar::TitleBar;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{ClosePanel, DockArea, DockItem};
use ui::modal::ModalButtonProps;
use ui::popover::{Popover, PopoverContent};
use ui::popup_menu::PopupMenuExt;
use ui::{h_flex, v_flex, ContextModal, IconName, Root, Sizable, StyledExt};

use crate::actions::{
    reset, DarkMode, KeyringPopup, Logout, Settings, Themes, ViewProfile, ViewRelays,
};
use crate::user::viewer;
use crate::views::compose::compose_button;
use crate::views::{onboarding, preferences, setup_relay, startup, welcome};
use crate::{login, new_identity, sidebar, user};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<ChatSpace> {
    cx.new(|cx| ChatSpace::new(window, cx))
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
    /// App's Title Bar
    title_bar: Entity<TitleBar>,

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

        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| DockArea::new(window, cx));
        let encryption_panel = encryption_ui::init(window, cx);

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

        subscriptions.push(
            // Observe the chat registry
            cx.observe(&chat, move |this, chat, cx| {
                let ids = this.get_all_panels(cx);

                chat.update(cx, |this, cx| {
                    this.refresh_rooms(ids, cx);
                });
            }),
        );

        Self {
            dock,
            title_bar,
            encryption_panel,
            ready: false,
            _subscriptions: subscriptions,
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

        self.ready = true;
        self.dock.update(cx, |this, cx| {
            this.set_left_dock(left, Some(px(DEFAULT_SIDEBAR_WIDTH)), true, window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn on_settings(&mut self, _ev: &Settings, window: &mut Window, cx: &mut Context<Self>) {
        let view = preferences::init(window, cx);

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title(SharedString::from("Preferences"))
                .width(px(520.))
                .child(view.clone())
        });
    }

    fn on_profile(&mut self, _ev: &ViewProfile, window: &mut Window, cx: &mut Context<Self>) {
        let view = user::init(window, cx);
        let entity = view.downgrade();

        window.open_modal(cx, move |modal, _window, _cx| {
            let entity = entity.clone();

            modal
                .title("Profile")
                .confirm()
                .child(view.clone())
                .button_props(ModalButtonProps::default().ok_text("Update"))
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
                                                window.close_all_modals(cx);
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

        window.open_modal(cx, move |this, _window, _cx| {
            let entity = entity.clone();

            this.confirm()
                .title(SharedString::from("Set Up Messaging Relays"))
                .child(view.clone())
                .button_props(ModalButtonProps::default().ok_text("Update"))
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

    fn on_themes(&mut self, _ev: &Themes, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, _window, cx| {
            let registry = ThemeRegistry::global(cx);
            let themes = registry.read(cx).themes();

            this.title("Select theme")
                .show_close(true)
                .overlay_closable(true)
                .child(v_flex().gap_2().pb_4().children({
                    let mut items = Vec::with_capacity(themes.len());

                    for (name, theme) in themes.iter() {
                        items.push(
                            h_flex()
                                .h_10()
                                .justify_between()
                                .child(
                                    v_flex()
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(cx.theme().text)
                                                .line_height(relative(1.3))
                                                .child(theme.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().text_muted)
                                                .child(theme.author.clone()),
                                        ),
                                )
                                .child(
                                    Button::new(format!("change-{name}"))
                                        .label("Set")
                                        .small()
                                        .ghost()
                                        .on_click({
                                            let theme = theme.clone();
                                            move |_ev, window, cx| {
                                                Theme::apply_theme(theme.clone(), Some(window), cx);
                                            }
                                        }),
                                ),
                        );
                    }

                    items
                }))
        })
    }

    fn on_sign_out(&mut self, _e: &Logout, _window: &mut Window, cx: &mut Context<Self>) {
        reset(cx);
    }

    fn on_open_pubkey(&mut self, ev: &OpenPublicKey, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = ev.0;
        let view = viewer::init(public_key, window, cx);

        window.open_modal(cx, move |this, _window, _cx| {
            this.alert()
                .show_close(true)
                .overlay_closable(true)
                .child(view.clone())
                .button_props(ModalButtonProps::default().ok_text("View on njump.me"))
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
        window.push_notification("Copied", cx);
    }

    fn on_keyring(&mut self, _ev: &KeyringPopup, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, _window, _cx| {
            this.show_close(true)
                .title(SharedString::from("Keyring is disabled"))
                .child(
                    v_flex()
                        .gap_2()
                        .pb_4()
                        .text_sm()
                        .child(SharedString::from("Coop cannot access the Keyring Service on your system. By design, Coop uses Keyring to store your credentials."))
                        .child(SharedString::from("Without access to Keyring, Coop will store your credentials as plain text."))
                        .child(SharedString::from("If you want to store your credentials in the Keyring, please enable Keyring and allow Coop to access it.")),
                )
        });
    }

    fn get_all_panels(&self, cx: &App) -> Option<Vec<u64>> {
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
                        .child(SharedString::from(
                            "Getting messages. This may take a while...",
                        )),
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
            .gap_2()
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
                        .bg(cx.theme().warning_background)
                        .text_color(cx.theme().warning_foreground)
                        .hover(|this| this.bg(cx.theme().warning_hover))
                        .active(|this| this.bg(cx.theme().warning_active))
                        .child(SharedString::from(format!(
                            "You have {} pending authentication requests",
                            pending_requests
                        )))
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
                                        .icon(IconName::Encryption)
                                        .rounded()
                                        .small()
                                        .cta()
                                        .map(|this| match has_encryption {
                                            true => this.ghost_alt(),
                                            false => this.warning(),
                                        }),
                                )
                                .content(move |window, cx| {
                                    let encryption_panel = encryption_panel.clone();

                                    cx.new(|cx| {
                                        PopoverContent::new(window, cx, move |_window, _cx| {
                                            if let Some(view) = encryption_panel.upgrade() {
                                                view.clone().into_any_element()
                                            } else {
                                                div().into_any_element()
                                            }
                                        })
                                    })
                                }),
                        )
                        .child(
                            Button::new("user")
                                .small()
                                .reverse()
                                .transparent()
                                .icon(IconName::CaretDown)
                                .child(Avatar::new(profile.avatar(proxy)).size(rems(1.45)))
                                .popup_menu(move |this, _window, _cx| {
                                    this.label(profile.display_name())
                                        .menu_with_icon(
                                            "Profile",
                                            IconName::EmojiFill,
                                            Box::new(ViewProfile),
                                        )
                                        .menu_with_icon(
                                            "Messaging Relays",
                                            IconName::Server,
                                            Box::new(ViewRelays),
                                        )
                                        .separator()
                                        .label(SharedString::from("Keyring Service"))
                                        .menu_with_icon_and_disabled(
                                            keyring_label.clone(),
                                            IconName::Encryption,
                                            Box::new(KeyringPopup),
                                            !is_using_file_keystore,
                                        )
                                        .separator()
                                        .menu_with_icon(
                                            "Dark Mode",
                                            IconName::Sun,
                                            Box::new(DarkMode),
                                        )
                                        .menu_with_icon("Themes", IconName::Moon, Box::new(Themes))
                                        .menu_with_icon(
                                            "Settings",
                                            IconName::Settings,
                                            Box::new(Settings),
                                        )
                                        .menu_with_icon(
                                            "Sign Out",
                                            IconName::Logout,
                                            Box::new(Logout),
                                        )
                                }),
                        ),
                )
            })
    }

    fn titlebar_center(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let panel = self.dock.read(cx).items.view();
        let title = panel.title(cx);
        let id = panel.panel_id(cx);

        if id == "Onboarding" {
            return div();
        };

        h_flex()
            .flex_1()
            .w_full()
            .justify_center()
            .text_center()
            .font_semibold()
            .text_sm()
            .child(
                div().flex_1().child(
                    Button::new("back")
                        .icon(IconName::ArrowLeft)
                        .small()
                        .ghost_alt()
                        .rounded()
                        .on_click(move |_ev, window, cx| {
                            entity
                                .update(cx, |this, cx| {
                                    this.set_onboarding_layout(window, cx);
                                })
                                .expect("Entity has been released");
                        }),
                ),
            )
            .child(div().flex_1().child(title))
            .child(div().flex_1())
    }
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        let left = self.titlebar_left(window, cx).into_any_element();
        let right = self.titlebar_right(window, cx).into_any_element();
        let center = self.titlebar_center(cx).into_any_element();
        let single_panel = self.dock.read(cx).items.panel_ids(cx).is_empty();

        // Update title bar children
        self.title_bar.update(cx, |this, _cx| {
            if single_panel {
                this.set_children(vec![center]);
            } else {
                this.set_children(vec![left, right]);
            }
        });

        div()
            .id(SharedString::from("chatspace"))
            .on_action(cx.listener(Self::on_settings))
            .on_action(cx.listener(Self::on_profile))
            .on_action(cx.listener(Self::on_relays))
            .on_action(cx.listener(Self::on_dark_mode))
            .on_action(cx.listener(Self::on_themes))
            .on_action(cx.listener(Self::on_sign_out))
            .on_action(cx.listener(Self::on_open_pubkey))
            .on_action(cx.listener(Self::on_copy_pubkey))
            .on_action(cx.listener(Self::on_keyring))
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
