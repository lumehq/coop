use std::sync::Arc;

use anyhow::Error;
use chats::{ChatRegistry, RoomEmitter};
use client_keys::ClientKeys;
use global::constants::{DEFAULT_MODAL_WIDTH, DEFAULT_SIDEBAR_WIDTH};
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, Action, App, AppContext, Axis, Context, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Task, Window,
};
use i18n::t;
use identity::Identity;
use nostr_connect::prelude::*;
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};
use theme::{ActiveTheme, Theme, ThemeMode};
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{DockArea, DockItem};
use ui::modal::ModalButtonProps;
use ui::{ContextModal, IconName, Root, Sizable, StyledExt, TitleBar};

use crate::views::chat::{self, Chat};
use crate::views::{login, new_account, onboarding, preferences, sidebar, startup, welcome};

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
    dock: Entity<DockArea>,
    toolbar: bool,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 4]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let dock = cx.new(|cx| {
            let panel = Arc::new(startup::init(window, cx));
            let center = DockItem::panel(panel);
            let mut dock = DockArea::new(window, cx);
            // Initialize the dock area with the center panel
            dock.set_center(center, window, cx);
            dock
        });

        cx.new(|cx| {
            let chats = ChatRegistry::global(cx);
            let client_keys = ClientKeys::global(cx);
            let identity = Identity::global(cx);
            let mut subscriptions = smallvec![];

            // Observe the client keys and show an alert modal if they fail to initialize
            subscriptions.push(cx.observe_in(
                &client_keys,
                window,
                |_this: &mut Self, state, window, cx| {
                    if !state.read(cx).has_keys() {
                        window.open_modal(cx, |this, _window, cx| {
                            this.overlay_closable(false)
                                .show_close(false)
                                .keyboard(false)
                                .confirm()
                                .button_props(
                                    ModalButtonProps::default()
                                        .cancel_text(t!("chatspace.create_new_keys"))
                                        .ok_text(t!("common.allow")),
                                )
                                .child(
                                    div()
                                        .px_10()
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
                                                .child(SharedString::new(t!("chatspace.warning"))),
                                        )
                                        .child(div().line_height(relative(1.4)).child(
                                            SharedString::new(t!(
                                                "chatspace.allow_keychain_access"
                                            )),
                                        )),
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
                    if !state.read(cx).has_profile() {
                        this.open_onboarding(window, cx);
                    } else {
                        // Load all chat rooms from database
                        ChatRegistry::global(cx).update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                        });
                        // Open chat panels
                        this.open_chats(window, cx);
                    }
                },
            ));

            // Automatically load messages when chat panel opens
            subscriptions.push(cx.observe_new::<Chat>(|this: &mut Chat, window, cx| {
                if let Some(window) = window {
                    this.load_messages(window, cx);
                }
            }));

            // Subscribe to open chat room requests
            subscriptions.push(cx.subscribe_in(
                &chats,
                window,
                |this: &mut Self, _state, event, window, cx| {
                    if let RoomEmitter::Open(room) = event {
                        if let Some(room) = room.upgrade() {
                            this.dock.update(cx, |this, cx| {
                                let panel = chat::init(room, window, cx);
                                let placement = DockPlacement::Center;

                                this.add_panel(panel, placement, window, cx);
                            });
                        } else {
                            window.push_notification(t!("chatspace.failed_to_open_room"), cx);
                        }
                    }
                },
            ));

            Self {
                dock,
                subscriptions,
                toolbar: false,
            }
        })
    }

    pub fn open_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // No active user, disable user's toolbar
        self.toolbar(false, cx);

        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    pub fn open_chats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Enable the toolbar for logged in users
        self.toolbar(true, cx);

        let weak_dock = self.dock.downgrade();
        let left = DockItem::panel(Arc::new(sidebar::init(window, cx)));
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

        self.dock.update(cx, |this, cx| {
            this.set_left_dock(left, Some(px(DEFAULT_SIDEBAR_WIDTH)), true, window, cx);
            this.set_center(center, window, cx);
        });

        cx.defer_in(window, |this, window, cx| {
            let verify_messaging_relays = this.verify_messaging_relays(cx);

            cx.spawn_in(window, async move |_, cx| {
                if let Ok(status) = verify_messaging_relays.await {
                    if !status {
                        cx.update(|window, cx| {
                            window.dispatch_action(
                                Box::new(ToggleModal {
                                    modal: ModalKind::SetupRelay,
                                }),
                                cx,
                            );
                        })
                        .ok();
                    }
                }
            })
            .detach();
        });
    }

    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = preferences::init(window, cx);

        window.open_modal(cx, move |modal, _, _| {
            modal
                .title(SharedString::new(t!("chatspace.preferences_title")))
                .width(px(DEFAULT_MODAL_WIDTH))
                .child(settings.clone())
        });
    }

    fn toolbar(&mut self, status: bool, cx: &mut Context<Self>) {
        self.toolbar = status;
        cx.notify();
    }

    fn verify_messaging_relays(&self, cx: &App) -> Task<Result<bool, Error>> {
        cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);
            let is_exist = client.database().query(filter).await?.first().is_some();

            Ok(is_exist)
        })
    }

    fn toggle_appearance(&self, window: &mut Window, cx: &mut App) {
        if cx.theme().mode.is_dark() {
            Theme::change(ThemeMode::Light, Some(window), cx);
        } else {
            Theme::change(ThemeMode::Dark, Some(window), cx);
        }
    }

    fn logout(&self, window: &mut Window, cx: &mut App) {
        Identity::global(cx).update(cx, |this, cx| {
            this.unload(window, cx);
        });
    }

    pub(crate) fn set_center_panel<P: PanelView>(panel: P, window: &mut Window, cx: &mut App) {
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

        div()
            .relative()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    // Title Bar
                    .child(
                        TitleBar::new()
                            // Left side
                            .child(div())
                            // Right side
                            .when(self.toolbar, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .gap_1p5()
                                        .px_2()
                                        .child(
                                            Button::new("appearance")
                                                .tooltip(t!("chatspace.appearance_tooltip"))
                                                .small()
                                                .ghost()
                                                .map(|this| {
                                                    if cx.theme().mode.is_dark() {
                                                        this.icon(IconName::Sun)
                                                    } else {
                                                        this.icon(IconName::Moon)
                                                    }
                                                })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.toggle_appearance(window, cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("preferences")
                                                .tooltip(t!("chatspace.preferences_tooltip"))
                                                .small()
                                                .ghost()
                                                .icon(IconName::Settings)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.open_settings(window, cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("logout")
                                                .tooltip(t!("common.logout"))
                                                .small()
                                                .ghost()
                                                .icon(IconName::Logout)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.logout(window, cx);
                                                })),
                                        ),
                                )
                            }),
                    )
                    // Dock
                    .child(self.dock.clone()),
            )
            // Notifications
            .child(div().absolute().top_8().children(notification_layer))
            // Modals
            .children(modal_layer)
    }
}
