use global::get_client;
use gpui::{
    actions, div, img, impl_internal_actions, prelude::FluentBuilder, px, App, AppContext, Axis,
    Context, Entity, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Styled,
    StyledImage, Window,
};
use serde::Deserialize;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::{dock::DockPlacement, DockArea, DockItem},
    popup_menu::PopupMenuExt,
    theme::{scale::ColorScaleStep, ActiveTheme, Appearance, Theme},
    ContextModal, Icon, IconName, Root, Sizable, TitleBar,
};

use super::{chat, contacts, onboarding, profile, relays, settings, sidebar, welcome};
use crate::device::Device;

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
actions!(account, [Logout]);

pub fn init(window: &mut Window, cx: &mut App) -> Entity<AppView> {
    AppView::new(window, cx)
}

pub struct AppView {
    dock: Entity<DockArea>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        // Initialize dock layout
        let dock = cx.new(|cx| DockArea::new(window, cx));
        let weak_dock = dock.downgrade();

        // Initialize left dock
        let left_panel = DockItem::panel(Arc::new(sidebar::init(window, cx)));

        // Initial central dock
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

        // Set default dock layout with left and central docks
        _ = weak_dock.update(cx, |view, cx| {
            view.set_left_dock(left_panel, Some(px(240.)), true, window, cx);
            view.set_center(center_panel, window, cx);
        });

        cx.new(|_| Self { dock })
    }

    fn render_mode_btn(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn render_account_btn(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("account")
            .ghost()
            .xsmall()
            .reverse()
            .icon(Icon::new(IconName::ChevronDownSmall))
            .when_some(Device::global(cx), |this, account| {
                this.when_some(account.read(cx).profile(), |this, profile| {
                    this.child(
                        img(profile.avatar.clone())
                            .size_5()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
            })
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

    fn render_relays_btn(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("relays")
            .xsmall()
            .ghost()
            .icon(IconName::Relays)
            .on_click(cx.listener(|this, _, window, cx| {
                this.render_edit_relays(window, cx);
            }))
    }

    fn render_edit_relays(&self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = relays::init(window, cx);

        window.open_modal(cx, move |this, window, cx| {
            let is_loading = relays.read(cx).loading();

            this.width(px(420.))
                .title("Edit your Messaging Relays")
                .child(relays.clone())
                .footer(
                    div()
                        .p_2()
                        .border_t_1()
                        .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                        .child(
                            Button::new("update_inbox_relays_btn")
                                .label("Update")
                                .primary()
                                .bold()
                                .rounded(ButtonRounded::Large)
                                .w_full()
                                .loading(is_loading)
                                .on_click(window.listener_for(&relays, |this, _, window, cx| {
                                    this.update(window, cx);
                                })),
                        ),
                )
        });
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
            PanelKind::Profile => {
                let panel = profile::init(window, cx);

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
        let client = get_client();

        cx.background_spawn(async move {
            // Reset nostr client
            client.reset().await
        })
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
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap_2()
                                    .px_2()
                                    .child(self.render_mode_btn(cx))
                                    .child(self.render_relays_btn(cx))
                                    .child(self.render_account_btn(cx)),
                            ),
                    )
                    // Dock
                    .child(self.dock.clone()),
            )
            // Notifications
            .child(div().absolute().top_8().children(notification_layer))
            // Modals
            .children(modal_layer)
            // Actions
            .on_action(cx.listener(Self::on_panel_action))
            .on_action(cx.listener(Self::on_logout_action))
    }
}
