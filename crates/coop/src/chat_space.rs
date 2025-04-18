use account::Account;
use gpui::{
    div, impl_internal_actions, prelude::FluentBuilder, px, App, AppContext, Axis, Context, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants},
    dock_area::{dock::DockPlacement, panel::PanelView, DockArea, DockItem},
    theme::{ActiveTheme, Appearance, Theme},
    ContextModal, IconName, Root, Sizable, TitleBar,
};

use crate::views::{chat, compose, contacts, profile, relays, welcome};
use crate::views::{onboarding, sidebar};

const MODAL_WIDTH: f32 = 420.;
const SIDEBAR_WIDTH: f32 = 280.;

impl_internal_actions!(dock, [AddPanel, ToggleModal]);

pub fn init(window: &mut Window, cx: &mut App) -> Entity<ChatSpace> {
    ChatSpace::new(window, cx)
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
    Contact,
    Relay,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ToggleModal {
    pub modal: ModalKind,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct AddPanel {
    panel: PanelKind,
    position: DockPlacement,
}

impl AddPanel {
    pub fn new(panel: PanelKind, position: DockPlacement) -> Self {
        Self { panel, position }
    }
}

pub struct ChatSpace {
    titlebar: bool,
    dock: Entity<DockArea>,
    #[allow(unused)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let account = Account::global(cx);
        let dock = cx.new(|cx| DockArea::new(window, cx));

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.observe_in(
                &account,
                window,
                |this: &mut ChatSpace, account, window, cx| {
                    if account.read(cx).profile.is_some() {
                        this.open_chats(window, cx);
                    } else {
                        this.open_onboarding(window, cx);
                    }
                },
            ));

            let mut this = Self {
                dock,
                subscriptions,
                titlebar: false,
            };

            if Account::global(cx).read(cx).profile.is_some() {
                this.open_chats(window, cx);
            } else {
                this.open_onboarding(window, cx);
            }

            this
        })
    }

    pub fn set_center_panel<P: PanelView>(panel: P, window: &mut Window, cx: &mut App) {
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

    fn open_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
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
            this.set_left_dock(left, Some(px(SIDEBAR_WIDTH)), true, window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn show_titlebar(&mut self, cx: &mut Context<Self>) {
        self.titlebar = true;
        cx.notify();
    }

    fn render_appearance_btn(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("appearance")
            .xsmall()
            .ghost()
            .map(|this| {
                if cx.theme().appearance.is_dark() {
                    this.icon(IconName::Sun)
                } else {
                    this.icon(IconName::Moon)
                }
            })
            .on_click(cx.listener(|_, _, window, cx| {
                if cx.theme().appearance.is_dark() {
                    Theme::change(Appearance::Light, Some(window), cx);
                } else {
                    Theme::change(Appearance::Dark, Some(window), cx);
                }
            }))
    }

    fn on_panel_action(&mut self, action: &AddPanel, window: &mut Window, cx: &mut Context<Self>) {
        match &action.panel {
            PanelKind::Room(id) => {
                // User must be logged in to open a room
                match chat::init(id, window, cx) {
                    Ok(panel) => {
                        self.dock.update(cx, |dock_area, cx| {
                            dock_area.add_panel(panel, action.position, window, cx);
                        });
                    }
                    Err(e) => window.push_notification(e.to_string(), cx),
                }
            }
        };
    }

    fn on_modal_action(
        &mut self,
        action: &ToggleModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.modal {
            ModalKind::Profile => {
                let profile = profile::init(window, cx);

                window.open_modal(cx, move |modal, _, _| {
                    modal
                        .title("Profile")
                        .width(px(MODAL_WIDTH))
                        .child(profile.clone())
                })
            }
            ModalKind::Compose => {
                let compose = compose::init(window, cx);

                window.open_modal(cx, move |modal, _, _| {
                    modal
                        .title("Direct Messages")
                        .width(px(MODAL_WIDTH))
                        .child(compose.clone())
                })
            }
            ModalKind::Contact => {
                let contacts = contacts::init(window, cx);

                window.open_modal(cx, move |this, _window, _cx| {
                    this.width(px(MODAL_WIDTH))
                        .title("Contacts")
                        .child(contacts.clone())
                });
            }
            ModalKind::Relay => {
                let relays = relays::init(window, cx);

                window.open_modal(cx, move |this, _, _| {
                    this.width(px(MODAL_WIDTH))
                        .title("Edit your Messaging Relays")
                        .child(relays.clone())
                });
            }
        };
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
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .gap_2()
                                        .px_2()
                                        .child(self.render_appearance_btn(cx)),
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
            .on_action(cx.listener(Self::on_panel_action))
            .on_action(cx.listener(Self::on_modal_action))
    }
}
