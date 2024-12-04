use components::{
    dock::{DockArea, DockItem},
    indicator::Indicator,
    theme::{ActiveTheme, Theme},
    Root, Sizable, TitleBar,
};
use gpui::*;
use std::sync::Arc;

use crate::states::account::AccountState;

use super::{
    block::{rooms::Rooms, welcome::WelcomeBlock, BlockContainer},
    onboarding::Onboarding,
};

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
    dock: Model<Option<View<DockArea>>>,
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
        let dock = cx.new_model(|_| None);
        let async_dock = dock.clone();

        // Observe UserState
        // If current user is present, fetching all gift wrap events
        cx.observe_global::<AccountState>(move |_, cx| {
            if cx.global::<AccountState>().in_use.is_some() {
                // Setup dock area
                let dock_area =
                    cx.new_view(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), cx));

                // Setup dock layout
                Self::init_layout(dock_area.downgrade(), cx);

                // Update dock model
                cx.update_model(&async_dock, |a, b| {
                    *a = Some(dock_area);
                    b.notify();
                });
            }
        })
        .detach();

        AppView { onboarding, dock }
    }

    fn init_layout(dock_area: WeakView<DockArea>, cx: &mut WindowContext) {
        let dock_item = Self::init_dock_items(&dock_area, cx);

        let left_panels = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(
                vec![Arc::new(BlockContainer::panel::<Rooms>(cx))],
                None,
                &dock_area,
                cx,
            )],
            vec![None, None],
            &dock_area,
            cx,
        );

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(DOCK_AREA.version, cx);
            view.set_left_dock(left_panels, Some(px(260.)), true, cx);
            view.set_center(dock_item, cx);
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
                vec![
                    Arc::new(BlockContainer::panel::<WelcomeBlock>(cx)),
                    // TODO: add chat block
                ],
                None,
                dock_area,
                cx,
            )],
            vec![None],
            dock_area,
            cx,
        )
    }
}

impl Render for AppView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(cx);
        let notification_layer = Root::render_notification_layer(cx);

        let mut content = div();

        if cx.global::<AccountState>().in_use.is_none() {
            content = content.size_full().child(self.onboarding.clone())
        } else {
            #[allow(clippy::collapsible_else_if)]
            if let Some(dock) = self.dock.read(cx).as_ref() {
                content = content
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(TitleBar::new())
                    .child(dock.clone())
            } else {
                content = content
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Indicator::new().small())
            }
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
