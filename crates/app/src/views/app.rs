use super::{chat::ChatPanel, sidebar::Sidebar, welcome::WelcomePanel};
use app_state::registry::AppRegistry;
use chat::registry::ChatRegistry;
use common::profile::NostrProfile;
use gpui::{
    actions, div, img, impl_internal_actions, px, AppContext, Axis, BorrowAppContext, Context,
    Edges, Entity, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Styled,
    StyledImage, Window,
};
use serde::Deserialize;
use state::get_client;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants},
    dock_area::{dock::DockPlacement, DockArea, DockItem},
    notification::NotificationType,
    popup_menu::PopupMenuExt,
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
    account: NostrProfile,
    dock: Entity<DockArea>,
}

impl AppView {
    pub fn new(account: NostrProfile, window: &mut Window, cx: &mut Context<'_, Self>) -> AppView {
        let dock = cx.new(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), window, cx));
        let weak_dock = dock.downgrade();

        let left = DockItem::panel(Arc::new(Sidebar::new(window, cx)));
        let center = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(
                vec![Arc::new(WelcomePanel::new(window, cx))],
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

        _ = weak_dock.update(cx, |view, cx| {
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

        AppView { account, dock }
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
                if let Some(weak_room) = cx.global::<ChatRegistry>().get_room(id, cx) {
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

    fn on_logout_action(&mut self, _action: &Logout, _window: &mut Window, cx: &mut Context<Self>) {
        cx.update_global::<AppRegistry, _>(|_this, cx| {
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
            .on_action(cx.listener(Self::on_profile_action))
            .on_action(cx.listener(Self::on_contacts_action))
            .on_action(cx.listener(Self::on_settings_action))
    }
}
