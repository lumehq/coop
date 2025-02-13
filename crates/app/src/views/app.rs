use cargo_packager_updater::{check_update, semver::Version, url::Url};
use common::{
    constants::{UPDATER_PUBKEY, UPDATER_URL},
    profile::NostrProfile,
};
use gpui::{
    actions, div, img, impl_internal_actions, prelude::FluentBuilder, px, App, AppContext, Axis,
    Context, Entity, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Styled,
    StyledImage, Window,
};
use log::info;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use state::get_client;
use std::sync::Arc;
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::{dock::DockPlacement, DockArea, DockItem},
    popup_menu::PopupMenuExt,
    theme::{scale::ColorScaleStep, ActiveTheme, Appearance, Theme},
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

impl_internal_actions!(dock, [AddPanel]);
actions!(account, [Logout]);

pub fn init(account: NostrProfile, window: &mut Window, cx: &mut App) -> Entity<AppView> {
    AppView::new(account, window, cx)
}

pub struct AppView {
    account: NostrProfile,
    relays: Entity<Option<Vec<String>>>,
    dock: Entity<DockArea>,
}

impl AppView {
    pub fn new(account: NostrProfile, window: &mut Window, cx: &mut App) -> Entity<Self> {
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

        // Check and auto update to the latest version
        cx.background_spawn(async move {
            // Set auto updater config
            let config = cargo_packager_updater::Config {
                endpoints: vec![Url::parse(UPDATER_URL).expect("Failed to parse UPDATER URL")],
                pubkey: String::from(UPDATER_PUBKEY),
                ..Default::default()
            };

            // Run auto updater
            if let Ok(current_version) = Version::parse(env!("CARGO_PKG_VERSION")) {
                if let Ok(Some(update)) = check_update(current_version, config) {
                    if update.download_and_install().is_ok() {
                        info!("Update installed")
                    }
                }
            }
        })
        .detach();

        cx.new(|cx| {
            let public_key = account.public_key();
            let relays = cx.new(|_| None);
            let async_relays = relays.downgrade();

            // Check user's messaging relays and determine user is ready for NIP17 or not.
            // If not, show the setup modal and instruct user setup inbox relays
            let client = get_client();
            let window_handle = window.window_handle();
            let (tx, rx) = oneshot::channel::<Option<Vec<String>>>();

            let this = Self {
                account,
                relays,
                dock,
            };

            cx.background_spawn(async move {
                let filter = Filter::new()
                    .kind(Kind::InboxRelays)
                    .author(public_key)
                    .limit(1);

                let relays = if let Ok(events) = client.database().query(filter).await {
                    if let Some(event) = events.first_owned() {
                        Some(
                            event
                                .tags
                                .filter_standardized(TagKind::Relay)
                                .filter_map(|t| match t {
                                    TagStandard::Relay(url) => Some(url.to_string()),

                                    _ => None,
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };

                _ = tx.send(relays);
            })
            .detach();

            cx.spawn(|this, mut cx| async move {
                if let Ok(result) = rx.await {
                    if let Some(relays) = result {
                        _ = cx.update(|cx| {
                            _ = async_relays.update(cx, |this, cx| {
                                *this = Some(relays);
                                cx.notify();
                            });
                        });
                    } else {
                        _ = cx.update_window(window_handle, |_, window, cx| {
                            this.update(cx, |this: &mut Self, cx| {
                                this.render_setup_relays(window, cx)
                            })
                        });
                    }
                }
            })
            .detach();

            this
        })
    }

    fn render_setup_relays(&self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = cx.new(|cx| Relays::new(None, window, cx));

        window.open_modal(cx, move |this, window, cx| {
            let is_loading = relays.read(cx).loading();

            this.keyboard(false)
                .closable(false)
                .width(px(420.))
                .title("Your Messaging Relays are not configured")
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

    fn render_edit_relay(&self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = self.relays.read(cx).clone();
        let view = cx.new(|cx| Relays::new(relays, window, cx));

        window.open_modal(cx, move |this, window, cx| {
            let is_loading = view.read(cx).loading();

            this.width(px(420.))
                .title("Edit your Messaging Relays")
                .child(view.clone())
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
                                .on_click(window.listener_for(&view, |this, _, window, cx| {
                                    this.update(window, cx);
                                })),
                        ),
                )
        });
    }

    fn render_appearance_button(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

    fn render_relays_button(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new("relays")
            .xsmall()
            .ghost()
            .icon(IconName::Relays)
            .on_click(cx.listener(|this, _, window, cx| {
                this.render_edit_relay(window, cx);
            }))
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
            PanelKind::Room(id) => match chat::init(id, window, cx) {
                Ok(panel) => {
                    self.dock.update(cx, |dock_area, cx| {
                        dock_area.add_panel(panel, action.position, window, cx);
                    });
                }
                Err(e) => window.push_notification(e.to_string(), cx),
            },
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
                            .gap_2()
                            .px_2()
                            .child(self.render_appearance_button(window, cx))
                            .child(self.render_relays_button(window, cx))
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
