use super::{
    account::Account, chat::ChatPanel, onboarding::Onboarding, sidebar::Sidebar,
    welcome::WelcomePanel,
};
use crate::states::{app::AppRegistry, chat::ChatRegistry};
use gpui::{
    div, impl_internal_actions, px, Axis, Context, Edges, InteractiveElement, IntoElement, Model,
    ParentElement, Render, Styled, View, ViewContext, VisualContext, WeakView, WindowContext,
};
use serde::Deserialize;
use std::sync::Arc;
use ui::{
    dock::{DockArea, DockItem, DockPlacement},
    indicator::Indicator,
    notification::NotificationType,
    theme::Theme,
    ContextModal, Root, Sizable, TitleBar,
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

impl_internal_actions!(dock, [AddPanel]);

pub struct DockAreaTab {
    id: &'static str,
    version: usize,
}

pub const DOCK_AREA: DockAreaTab = DockAreaTab {
    id: "dock",
    version: 1,
};

pub struct AppView {
    account: Model<Option<View<Account>>>,
    onboarding: View<Onboarding>,
    dock: View<DockArea>,
}

impl AppView {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> AppView {
        // Sync theme with system
        cx.observe_window_appearance(|_, cx| {
            Theme::sync_system_appearance(cx);
        })
        .detach();

        // Account
        let account = cx.new_model(|_| None);
        let async_account = account.clone();

        // Onboarding
        let onboarding = cx.new_view(Onboarding::new);

        // Dock
        let dock = cx.new_view(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), cx));

        // Get current user from app state
        let current_user = cx.global::<AppRegistry>().current_user();

        if let Some(current_user) = current_user.upgrade() {
            cx.observe(&current_user, move |view, model, cx| {
                if let Some(public_key) = model.read(cx).clone().as_ref() {
                    Self::init_layout(view.dock.downgrade(), cx);
                    // TODO: save dock state and load previous state on startup

                    let view = cx.new_view(|cx| {
                        let view = Account::new(*public_key, cx);
                        // Initial load metadata
                        view.load_metadata(cx);

                        view
                    });

                    cx.update_model(&async_account, |model, cx| {
                        *model = Some(view);
                        cx.notify();
                    });
                }
            })
            .detach();
        }

        AppView {
            account,
            onboarding,
            dock,
        }
    }

    fn init_layout(dock_area: WeakView<DockArea>, cx: &mut WindowContext) {
        let left = DockItem::panel(Arc::new(Sidebar::new(cx)));
        let center = Self::init_dock_items(&dock_area, cx);

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

    fn init_dock_items(dock_area: &WeakView<DockArea>, cx: &mut WindowContext) -> DockItem {
        DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(
                vec![Arc::new(WelcomePanel::new(cx))],
                None,
                dock_area,
                cx,
            )],
            vec![None],
            dock_area,
            cx,
        )
    }

    fn on_action_add_panel(&mut self, action: &AddPanel, cx: &mut ViewContext<Self>) {
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
}

impl Render for AppView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(cx);
        let notification_layer = Root::render_notification_layer(cx);

        let mut content = div().size_full().flex().flex_col();

        if cx.global::<AppRegistry>().is_loading {
            content = content.child(div()).child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Indicator::new().small()),
            )
        } else if let Some(account) = self.account.read(cx).as_ref() {
            content = content
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
                                .child(account.clone()),
                        ),
                )
                .child(self.dock.clone())
                .on_action(cx.listener(Self::on_action_add_panel))
        } else {
            content = content
                .child(TitleBar::new())
                .child(self.onboarding.clone())
        }

        div()
            .size_full()
            .child(content)
            .child(div().absolute().top_8().children(notification_layer))
            .children(modal_layer)
    }
}
