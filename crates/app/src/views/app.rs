use coop_ui::{
    dock::{DockArea, DockItem, DockPlacement},
    theme::Theme,
    Root, TitleBar,
};
use gpui::*;
use prelude::FluentBuilder;
use serde::Deserialize;
use std::sync::Arc;

use super::{
    account::Account, chat::ChatPanel, contact::ContactPanel, onboarding::Onboarding,
    sidebar::Sidebar, welcome::WelcomePanel,
};
use crate::states::{account::AccountRegistry, chat::Room};

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub enum PanelKind {
    Room(Arc<Room>),
    Contact,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct AddPanel {
    pub panel: PanelKind,
    pub position: DockPlacement,
}

impl_actions!(dock, [AddPanel]);

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

        cx.observe_global::<AccountRegistry>(move |view, cx| {
            if let Some(public_key) = cx.global::<AccountRegistry>().get() {
                Self::init_layout(view.dock.downgrade(), cx);
                // TODO: save dock state and load previous state on startup

                let view = cx.new_view(|cx| Account::new(public_key, cx));

                cx.update_model(&async_account, |model, cx| {
                    *model = Some(view);
                    cx.notify();
                });
            }
        })
        .detach();

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
            PanelKind::Room(room) => {
                let panel = Arc::new(ChatPanel::new(room, cx));

                self.dock.update(cx, |dock_area, cx| {
                    dock_area.add_panel(panel, action.position, cx);
                });
            }
            PanelKind::Contact => {
                let panel = Arc::new(ContactPanel::new(cx));

                self.dock.update(cx, |dock_area, cx| {
                    dock_area.add_panel(panel, action.position, cx);
                });
            }
        };
    }
}

impl Render for AppView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(cx);
        let notification_layer = Root::render_notification_layer(cx);

        let mut content = div().size_full().flex().flex_col();

        if cx.global::<AccountRegistry>().is_user_logged_in() {
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
                                .when_some(self.account.read(cx).as_ref(), |this, account| {
                                    this.child(account.clone())
                                }),
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
