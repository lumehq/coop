use super::{chat::ChatPanel, onboarding::Onboarding, sidebar::Sidebar, welcome::WelcomePanel};
use gpui::{
    actions, div, img, impl_internal_actions, px, svg, App, AppContext, Axis, BorrowAppContext,
    Context, Edges, Entity, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render,
    Styled, StyledImage, WeakEntity, Window,
};
use registry::{app::AppRegistry, chat::ChatRegistry, contact::Contact};
use serde::Deserialize;
use state::get_client;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants},
    dock_area::{dock::DockPlacement, DockArea, DockItem},
    notification::NotificationType,
    popup_menu::PopupMenuExt,
    prelude::FluentBuilder,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Icon, IconName, Root, Sizable, TitleBar,
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub enum PanelKind {
    Room(u64),
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct AddPanel {
    pub panel: PanelKind,
    pub position: DockPlacement,
}

// Dock actions
impl_internal_actions!(dock, [AddPanel]);
// Account actions
actions!(account, [OpenProfile, OpenContacts, OpenSettings, Logout]);

pub struct DockAreaTab {
    id: &'static str,
    version: usize,
}

pub const DOCK_AREA: DockAreaTab = DockAreaTab {
    id: "dock",
    version: 1,
};

pub struct AppView {
    onboarding: Entity<Onboarding>,
    dock: Entity<DockArea>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> AppView {
        let onboarding = cx.new(|cx| Onboarding::new(window, cx));
        let dock = cx.new(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), window, cx));

        // Get current user from app state
        let weak_user = cx.global::<AppRegistry>().user();

        if let Some(user) = weak_user.upgrade() {
            cx.observe_in(&user, window, |view, this, window, cx| {
                if this.read(cx).is_some() {
                    Self::render_dock(view.dock.downgrade(), window, cx);
                }
            })
            .detach();
        }

        AppView { onboarding, dock }
    }

    fn render_dock(dock_area: WeakEntity<DockArea>, window: &mut Window, cx: &mut App) {
        let left = DockItem::panel(Arc::new(Sidebar::new(window, cx)));
        let center = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(
                vec![Arc::new(WelcomePanel::new(window, cx))],
                None,
                &dock_area,
                window,
                cx,
            )],
            vec![None],
            &dock_area,
            window,
            cx,
        );

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(DOCK_AREA.version, window, cx);
            view.set_left_dock(left, Some(px(240.)), true, window, cx);
            view.set_center(center, window, cx);
            view.set_dock_collapsible(
                Edges {
                    left: false,
                    ..Default::default()
                },
                window,
                cx,
            );
            // TODO: support right dock?
            // TODO: support bottom dock?
        });
    }

    fn render_account(&self, account: Contact) -> impl IntoElement {
        Button::new("account")
            .ghost()
            .xsmall()
            .reverse()
            .icon(Icon::new(IconName::ChevronDownSmall))
            .child(
                img(account.avatar())
                    .size_5()
                    .rounded_full()
                    .object_fit(ObjectFit::Cover),
            )
            .popup_menu(move |this, _, _cx| {
                this.menu("Profile", Box::new(OpenProfile))
                    .menu("Contacts", Box::new(OpenContacts))
                    .menu("Settings", Box::new(OpenSettings))
                    .separator()
                    .menu("Change account", Box::new(Logout))
            })
    }

    fn on_panel_action(&mut self, action: &AddPanel, window: &mut Window, cx: &mut Context<Self>) {
        match &action.panel {
            PanelKind::Room(id) => {
                if let Some(weak_room) = cx.global::<ChatRegistry>().room(id, cx) {
                    if let Some(room) = weak_room.upgrade() {
                        let panel = Arc::new(ChatPanel::new(room, window, cx));

                        self.dock.update(cx, |dock_area, cx| {
                            dock_area.add_panel(panel, action.position, window, cx);
                        });
                    } else {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                "System error. Cannot open this chat room.",
                            ),
                            cx,
                        );
                    }
                }
            }
        };
    }

    fn on_profile_action(
        &mut self,
        _action: &OpenProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // TODO
    }

    fn on_contacts_action(
        &mut self,
        _action: &OpenContacts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // TODO
    }

    fn on_settings_action(
        &mut self,
        _action: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // TODO
    }

    fn on_logout_action(&mut self, _action: &Logout, window: &mut Window, cx: &mut Context<Self>) {
        cx.update_global::<AppRegistry, _>(|this, cx| {
            this.logout(cx);
            // Reset nostr client
            cx.background_executor()
                .spawn(async move { get_client().reset().await })
                .detach();
        });
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let state = cx.global::<AppRegistry>();

        div()
            .size_full()
            .flex()
            .flex_col()
            // Main
            .map(|this| {
                if state.is_loading {
                    this
                        // Placeholder
                        .child(div())
                        .child(
                            div().flex_1().flex().items_center().justify_center().child(
                                svg()
                                    .path("brand/coop.svg")
                                    .size_12()
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::THREE)),
                            ),
                        )
                } else if let Some(contact) = state.current_user(window, cx) {
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
                                    .gap_1()
                                    .px_2()
                                    .child(self.render_account(contact)),
                            ),
                    )
                    .child(self.dock.clone())
                    // Listener
                    .on_action(cx.listener(Self::on_panel_action))
                    .on_action(cx.listener(Self::on_logout_action))
                    .on_action(cx.listener(Self::on_profile_action))
                    .on_action(cx.listener(Self::on_contacts_action))
                    .on_action(cx.listener(Self::on_settings_action))
                } else {
                    this.child(TitleBar::new()).child(self.onboarding.clone())
                }
            })
            // Notification
            .child(div().absolute().top_8().children(notification_layer))
            // Modal
            .children(modal_layer)
    }
}
