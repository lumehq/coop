use std::sync::Arc;

use components::{
    dock::{DockArea, DockItem},
    theme::{ActiveTheme, Theme},
    TitleBar,
};
use gpui::*;

use super::{
    block::{welcome::WelcomeBlock, BlockContainer},
    onboarding::Onboarding,
};
use crate::state::AppState;

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
    dock_area: View<DockArea>,
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
        let dock_area = cx.new_view(|cx| DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), cx));
        let weak_dock_area = dock_area.downgrade();

        // Set dock layout
        Self::init_layout(weak_dock_area, cx);

        AppView {
            onboarding,
            dock_area,
        }
    }

    fn init_layout(dock_area: WeakView<DockArea>, cx: &mut WindowContext) {
        let dock_item = Self::init_dock_items(&dock_area, cx);
        let left_panels =
            DockItem::split_with_sizes(Axis::Vertical, vec![], vec![None, None], &dock_area, cx);

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(DOCK_AREA.version, cx);
            view.set_left_dock(left_panels, Some(px(260.)), true, cx);
            view.set_root(dock_item, cx);
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
        let mut content = div();

        if cx.global::<AppState>().signer.is_none() {
            content = content.child(self.onboarding.clone())
        } else {
            content = content
                .size_full()
                .flex()
                .flex_col()
                .child(TitleBar::new())
                .child(self.dock_area.clone())
        }

        div()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .size_full()
            .child(content)
    }
}
