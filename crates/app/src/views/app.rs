use coop_ui::{
    dock::{DockArea, DockItem, DockPlacement},
    theme::{ActiveTheme, Theme},
    Root, TitleBar,
};
use gpui::*;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

use super::{
    dock::{chat::ChatPanel, left_dock::LeftDock, welcome::WelcomePanel},
    onboarding::Onboarding,
};
use crate::states::account::AccountRegistry;

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct AddPanel {
    pub title: Option<String>,
    pub receiver: PublicKey,
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

        // Onboarding
        let onboarding = cx.new_view(Onboarding::new);

        // Dock
        let dock = cx.new_view(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), cx));

        cx.observe_global::<AccountRegistry>(|view, cx| {
            // TODO: save dock state and load previous state on startup
            if cx.global::<AccountRegistry>().is_user_logged_in() {
                Self::init_layout(view.dock.downgrade(), cx);
            }
        })
        .detach();

        AppView { onboarding, dock }
    }

    fn init_layout(dock_area: WeakView<DockArea>, cx: &mut WindowContext) {
        let left = DockItem::panel(Arc::new(LeftDock::new(cx)));
        let center = Self::init_dock_items(&dock_area, cx);

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(DOCK_AREA.version, cx);
            view.set_left_dock(left, Some(px(260.)), true, cx);
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
        let chat_panel = Arc::new(ChatPanel::new(action.receiver, cx));

        self.dock.update(cx, |dock_area, cx| {
            dock_area.add_panel(chat_panel, DockPlacement::Center, cx);
        });
    }
}

impl Render for AppView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(cx);
        let notification_layer = Root::render_notification_layer(cx);

        let mut content = div();

        if cx.global::<AccountRegistry>().is_user_logged_in() {
            content = content
                .on_action(cx.listener(Self::on_action_add_panel))
                .size_full()
                .flex()
                .flex_col()
                .child(TitleBar::new())
                .child(self.dock.clone())
        } else {
            content = content.size_full().child(self.onboarding.clone())
        }

        div()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .size_full()
            .child(content)
            .child(div().absolute().top_8().children(notification_layer))
            .children(modal_layer)
    }
}
