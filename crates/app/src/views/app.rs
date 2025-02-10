use chat_state::registry::ChatRegistry;
use common::profile::NostrProfile;
use gpui::{
    actions, div, img, impl_internal_actions, px, App, AppContext, Axis, Context, Entity,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Styled, StyledImage, Window,
};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use state::get_client;
use std::sync::Arc;
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::{dock::DockPlacement, DockArea, DockItem},
    popup_menu::PopupMenuExt,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Icon, IconName, Root, Sizable, TitleBar,
};

use super::{chat, contacts, onboarding, profile, relays::Relays, settings, sidebar, welcome};

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub enum PanelKind {
    Room(u64),
    Profile,
    Contacts,
    Settings,
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

// Dock actions
impl_internal_actions!(dock, [AddPanel]);

// Account actions
actions!(account, [OpenProfile, OpenContacts, OpenSettings, Logout]);

pub fn init(account: NostrProfile, window: &mut Window, cx: &mut App) -> Entity<AppView> {
    AppView::new(account, window, cx)
}

pub struct AppView {
    account: NostrProfile,
    dock: Entity<DockArea>,
}

impl AppView {
    pub fn new(account: NostrProfile, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let dock = cx.new(|cx| DockArea::new(window, cx));
        let weak_dock = dock.downgrade();
        let left_panel = DockItem::panel(Arc::new(sidebar::init(window, cx)));
        let center_panel = DockItem::split_with_sizes(
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

        // Set default dock layout
        _ = weak_dock.update(cx, |view, cx| {
            view.set_left_dock(left_panel, Some(px(240.)), true, window, cx);
            view.set_center(center_panel, window, cx);
        });

        let public_key = account.public_key();
        let window_handle = window.window_handle();

        // Check user's inbox relays and determine user is ready for NIP17 or not.
        // If not, show the setup modal and instruct user setup inbox relays
        cx.spawn(|mut cx| async move {
            let (tx, rx) = oneshot::channel::<bool>();

            cx.background_spawn(async move {
                let client = get_client();
                let filter = Filter::new()
                    .kind(Kind::InboxRelays)
                    .author(public_key)
                    .limit(1);

                let is_ready = if let Ok(events) = client.database().query(filter).await {
                    events.first_owned().is_some()
                } else {
                    false
                };

                _ = tx.send(is_ready);
            })
            .detach();

            if let Ok(is_ready) = rx.await {
                if is_ready {
                    //
                } else {
                    cx.update_window(window_handle, |_, window, cx| {
                        let relays = cx.new(|cx| Relays::new(window, cx));

                        window.open_modal(cx, move |this, window, cx| {
                            let is_loading = relays.read(cx).loading();

                            this.keyboard(false)
                                .closable(false)
                                .width(px(420.))
                                .title("Your Inbox is not configured")
                                .child(relays.clone())
                                .footer(
                                    div()
                                        .p_2()
                                        .border_t_1()
                                        .border_color(
                                            cx.theme().base.step(cx, ColorScaleStep::FIVE),
                                        )
                                        .child(
                                            Button::new("update_inbox_relays_btn")
                                                .label("Update")
                                                .primary()
                                                .bold()
                                                .rounded(ButtonRounded::Large)
                                                .w_full()
                                                .loading(is_loading)
                                                .on_click(window.listener_for(
                                                    &relays,
                                                    |this, _, window, cx| {
                                                        this.update(window, cx);
                                                    },
                                                )),
                                        ),
                                )
                        });
                    })
                    .unwrap();
                }
            }
        })
        .detach();

        cx.new(|_| Self { account, dock })
    }

    fn render_account(&self) -> impl IntoElement {
        Button::new("account")
            .ghost()
            .xsmall()
            .reverse()
            .icon(Icon::new(IconName::ChevronDownSmall))
            .child(
                img(self.account.avatar())
                    .size_5()
                    .rounded_full()
                    .object_fit(ObjectFit::Cover),
            )
            .popup_menu(move |this, _, _cx| {
                this.menu(
                    "Profile",
                    Box::new(AddPanel::new(PanelKind::Profile, DockPlacement::Right)),
                )
                .menu(
                    "Contacts",
                    Box::new(AddPanel::new(PanelKind::Contacts, DockPlacement::Right)),
                )
                .menu(
                    "Settings",
                    Box::new(AddPanel::new(PanelKind::Settings, DockPlacement::Center)),
                )
                .separator()
                .menu("Change account", Box::new(Logout))
            })
    }

    fn on_panel_action(&mut self, action: &AddPanel, window: &mut Window, cx: &mut Context<Self>) {
        match &action.panel {
            PanelKind::Room(id) => {
                if let Some(weak_room) = cx.global::<ChatRegistry>().get_room(id, cx) {
                    if let Some(room) = weak_room.upgrade() {
                        let panel = Arc::new(chat::init(&room, window, cx));

                        self.dock.update(cx, |dock_area, cx| {
                            dock_area.add_panel(panel, action.position, window, cx);
                        });
                    }
                }
            }
            PanelKind::Profile => {
                let panel = Arc::new(profile::init(self.account.clone(), window, cx));

                self.dock.update(cx, |dock_area, cx| {
                    dock_area.add_panel(panel, action.position, window, cx);
                });
            }
            PanelKind::Contacts => {
                let panel = Arc::new(contacts::init(window, cx));

                self.dock.update(cx, |dock_area, cx| {
                    dock_area.add_panel(panel, action.position, window, cx);
                });
            }
            PanelKind::Settings => {
                let panel = Arc::new(settings::init(window, cx));

                self.dock.update(cx, |dock_area, cx| {
                    dock_area.add_panel(panel, action.position, window, cx);
                });
            }
        };
    }

    fn on_logout_action(&mut self, _action: &Logout, window: &mut Window, cx: &mut Context<Self>) {
        cx.background_spawn(async move { get_client().reset().await })
            .detach();

        window.replace_root(cx, |window, cx| {
            Root::new(onboarding::init(window, cx).into(), window, cx)
        });
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            // Main
            .child(
                TitleBar::new()
                    // Left side
                    .child(div())
                    // Right side
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_1()
                            .px_2()
                            .child(self.render_account()),
                    ),
            )
            .child(self.dock.clone())
            .child(div().absolute().top_8().children(notification_layer))
            .children(modal_layer)
            .on_action(cx.listener(Self::on_panel_action))
            .on_action(cx.listener(Self::on_logout_action))
    }
}
