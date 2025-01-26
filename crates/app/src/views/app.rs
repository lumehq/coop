use super::{chat::ChatPanel, onboarding::Onboarding, sidebar::Sidebar, welcome::WelcomePanel};
use gpui::{
    actions, div, img, impl_internal_actions, px, svg, Axis, BorrowAppContext, Edges,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Styled, StyledImage, View,
    ViewContext, VisualContext, WeakView, WindowContext,
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
    onboarding: View<Onboarding>,
    dock: View<DockArea>,
}

impl AppView {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> AppView {
        let onboarding = cx.new_view(Onboarding::new);
        let dock = cx.new_view(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), cx));

        // Get current user from app state
        let weak_user = cx.global::<AppRegistry>().user();

        if let Some(user) = weak_user.upgrade() {
            cx.observe(&user, move |view, this, cx| {
                if this.read(cx).is_some() {
                    Self::render_dock(view.dock.downgrade(), cx);
                }
            })
            .detach();
        }

        AppView { onboarding, dock }
    }

    fn render_dock(dock_area: WeakView<DockArea>, cx: &mut WindowContext) {
        let left = DockItem::panel(Arc::new(Sidebar::new(cx)));
        let center = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(
                vec![Arc::new(WelcomePanel::new(cx))],
                None,
                &dock_area,
                cx,
            )],
            vec![None],
            &dock_area,
            cx,
        );

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(DOCK_AREA.version, cx);
            view.set_left_dock(left, Some(px(240.)), true, cx);
            view.set_center(center, cx);
            view.set_dock_collapsible(
                Edges {
                    left: false,
                    ..Default::default()
                },
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
            .popup_menu(move |this, _cx| {
                this.menu("Profile", Box::new(OpenProfile))
                    .menu("Contacts", Box::new(OpenContacts))
                    .menu("Settings", Box::new(OpenSettings))
                    .separator()
                    .menu("Change account", Box::new(Logout))
            })
    }

    fn on_panel_action(&mut self, action: &AddPanel, cx: &mut ViewContext<Self>) {
        match &action.panel {
            PanelKind::Room(id) => {
                if let Some(weak_room) = cx.global::<ChatRegistry>().room(id, cx) {
                    if let Some(room) = weak_room.upgrade() {
                        let panel = Arc::new(ChatPanel::new(room, cx));

                        self.dock.update(cx, |dock_area, cx| {
                            dock_area.add_panel(panel, action.position, cx);
                        });
                    } else {
                        cx.push_notification((
                            NotificationType::Error,
                            "System error. Cannot open this chat room.",
                        ));
                    }
                }
            }
        };
    }

    fn on_profile_action(&mut self, _action: &OpenProfile, cx: &mut ViewContext<Self>) {
        // TODO
    }

    fn on_contacts_action(&mut self, _action: &OpenContacts, cx: &mut ViewContext<Self>) {
        // TODO
    }

    fn on_settings_action(&mut self, _action: &OpenSettings, cx: &mut ViewContext<Self>) {
        // TODO
    }

    fn on_logout_action(&mut self, _action: &Logout, cx: &mut ViewContext<Self>) {
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
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(cx);
        let notification_layer = Root::render_notification_layer(cx);
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
                } else if let Some(contact) = state.current_user(cx) {
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
