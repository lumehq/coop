use std::sync::Arc;

use anyhow::Error;
use app_state::AppState;
use chats::{ChatRegistry, RoomEmitter};
use global::constants::{DEFAULT_MODAL_WIDTH, DEFAULT_SIDEBAR_WIDTH};
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, impl_internal_actions, px, App, AppContext, Axis, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, Task, Window,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};
use theme::{ActiveTheme, Theme, ThemeMode};
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{DockArea, DockItem};
use ui::{ContextModal, IconName, Root, Sizable, TitleBar};

use crate::views::chat::{self, Chat};
use crate::views::{compose, login, new_account, onboarding, profile, relays, sidebar, welcome};

impl_internal_actions!(dock, [ToggleModal]);

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

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ToggleModal {
    pub modal: ModalKind,
}

pub struct ChatSpace {
    titlebar: bool,
    dock: Entity<DockArea>,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 3]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let dock = cx.new(|cx| {
            let panel = Arc::new(onboarding::init(window, cx));
            let center = DockItem::panel(panel);
            let mut dock = DockArea::new(window, cx);
            // Initialize the dock area with the center panel
            dock.set_center(center, window, cx);
            dock
        });

        cx.new(|cx| {
            let account = AppState::global(cx);
            let chats = ChatRegistry::global(cx);
            let mut subscriptions = smallvec![];

            subscriptions.push(
                cx.observe_in(&account, window, |this: &mut ChatSpace, account, window, cx| {
                    if account.read(cx).account().is_some() {
                        this.open_chats(window, cx);
                    } else {
                        this.open_onboarding(window, cx);
                    }
                }),
            );

            subscriptions.push(cx.subscribe_in(&chats, window, |this, _state, event, window, cx| {
                if let RoomEmitter::Open(room) = event {
                    if let Some(room) = room.upgrade() {
                        this.dock.update(cx, |this, cx| {
                            let panel = chat::init(room, window, cx);
                            this.add_panel(panel, DockPlacement::Center, window, cx);
                        });
                    } else {
                        window.push_notification("Failed to open room. Please retry later.", cx);
                    }
                }
            }));

            subscriptions.push(cx.observe_new::<Chat>(|this, window, cx| {
                if let Some(window) = window {
                    this.load_messages(window, cx);
                }
            }));

            Self {
                dock,
                subscriptions,
                titlebar: false,
            }
        })
    }

    fn show_titlebar(&mut self, cx: &mut Context<Self>) {
        self.titlebar = true;
        cx.notify();
    }

    fn open_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn open_chats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_titlebar(cx);

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

    fn verify_messaging_relays(&self, cx: &App) -> Task<Result<bool, Error>> {
        cx.background_spawn(async move {
            let signer = shared_state().client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let filter = Filter::new().kind(Kind::InboxRelays).author(public_key).limit(1);
            let is_exist = shared_state().client.database().query(filter).await?.first().is_some();

            Ok(is_exist)
        })
    }

    fn on_modal_action(&mut self, action: &ToggleModal, window: &mut Window, cx: &mut Context<Self>) {
        match action.modal {
            ModalKind::Profile => {
                let profile = profile::init(window, cx);

                window.open_modal(cx, move |modal, _, _| {
                    modal
                        .title("Profile")
                        .width(px(DEFAULT_MODAL_WIDTH))
                        .child(profile.clone())
                })
            }
            ModalKind::Compose => {
                let compose = compose::init(window, cx);

                window.open_modal(cx, move |modal, _, _| {
                    modal
                        .title("Direct Messages")
                        .width(px(DEFAULT_MODAL_WIDTH))
                        .child(compose.clone())
                })
            }
            ModalKind::Relay => {
                let relays = relays::init(window, cx);

                window.open_modal(cx, move |this, _, _| {
                    this.width(px(DEFAULT_MODAL_WIDTH))
                        .title("Edit your Messaging Relays")
                        .child(relays.clone())
                });
            }
            ModalKind::SetupRelay => {
                let relays = relays::init(window, cx);

                window.open_modal(cx, move |this, _, _| {
                    this.width(px(DEFAULT_MODAL_WIDTH))
                        .title("Your Messaging Relays are not configured")
                        .child(relays.clone())
                });
            }
            _ => {}
        };
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
                    .when(self.titlebar, |this| {
                        this.child(
                            TitleBar::new()
                                // Left side
                                .child(div())
                                // Right side
                                .child(
                                    div().flex().items_center().justify_end().gap_2().px_2().child(
                                        Button::new("appearance")
                                            .xsmall()
                                            .ghost()
                                            .map(|this| {
                                                if cx.theme().mode.is_dark() {
                                                    this.icon(IconName::Sun)
                                                } else {
                                                    this.icon(IconName::Moon)
                                                }
                                            })
                                            .on_click(cx.listener(|_, _, window, cx| {
                                                if cx.theme().mode.is_dark() {
                                                    Theme::change(ThemeMode::Light, Some(window), cx);
                                                } else {
                                                    Theme::change(ThemeMode::Dark, Some(window), cx);
                                                }
                                            })),
                                    ),
                                ),
                        )
                    })
                    // Dock
                    .child(self.dock.clone()),
            )
            // Notifications
            .child(div().absolute().top_8().children(notification_layer))
            // Modals
            .children(modal_layer)
            // Actions
            .on_action(cx.listener(Self::on_modal_action))
    }
}
