use components::{
    dock::{DockArea, DockItem, PanelStyle},
    theme::{ActiveTheme, Theme},
    Root, TitleBar,
};
use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::{cmp::Reverse, sync::Arc, time::Duration};

use crate::{
    get_client,
    states::{
        room::{Room, RoomLastMessage, Rooms},
        user::UserState,
    },
};

use super::{
    block::{welcome::WelcomeBlock, BlockContainer},
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
    dock_area: View<DockArea>,
}

impl AppView {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> AppView {
        // Sync theme with system
        cx.observe_window_appearance(|_, cx| {
            Theme::sync_system_appearance(cx);
        })
        .detach();

        // Observe UserState
        // If current user is present, fetching all gift wrap events
        cx.observe_global::<UserState>(|_v, cx| {
            let app_state = cx.global::<UserState>();
            let view_id = cx.parent_view_id();
            let mut async_cx = cx.to_async();

            if let Some(public_key) = app_state.current_user {
                cx.foreground_executor()
                    .spawn(async move {
                        let client = get_client().await;
                        let filter = Filter::new().pubkey(public_key).kind(Kind::GiftWrap);

                        let mut rumors: Vec<UnsignedEvent> = Vec::new();

                        if let Ok(mut rx) = client
                            .stream_events(vec![filter], Some(Duration::from_secs(30)))
                            .await
                        {
                            while let Some(event) = rx.next().await {
                                if let Ok(UnwrappedGift { rumor, .. }) =
                                    client.unwrap_gift_wrap(&event).await
                                {
                                    rumors.push(rumor);
                                };
                            }

                            let items = rumors
                                .into_iter()
                                .sorted_by_key(|ev| Reverse(ev.created_at))
                                .filter(|ev| ev.pubkey != public_key)
                                .unique_by(|ev| ev.pubkey)
                                .map(|item| {
                                    Room::new(
                                        vec![item.pubkey, public_key],
                                        Some(RoomLastMessage {
                                            content: Some(item.content),
                                            time: item.created_at,
                                        }),
                                    )
                                })
                                .collect::<Vec<_>>();

                            _ = async_cx.update_global::<Rooms, _>(|state, cx| {
                                state.rooms = items;
                                cx.notify(view_id);
                            });
                        }
                    })
                    .detach();
            }
        })
        .detach();

        // Onboarding
        let onboarding = cx.new_view(Onboarding::new);

        // Dock
        let dock_area = cx.new_view(|cx| {
            DockArea::new(DOCK_AREA.id, Some(DOCK_AREA.version), cx).panel_style(PanelStyle::TabBar)
        });

        // Set dock layout
        Self::init_layout(dock_area.downgrade(), cx);

        AppView {
            onboarding,
            dock_area,
        }
    }

    fn init_layout(dock_area: WeakView<DockArea>, cx: &mut WindowContext) {
        let dock_item = Self::init_dock_items(&dock_area, cx);

        let left_panels = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(vec![], None, &dock_area, cx)],
            vec![None, None],
            &dock_area,
            cx,
        );

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(DOCK_AREA.version, cx);
            view.set_left_dock(left_panels, Some(px(260.)), true, cx);
            view.set_root(dock_item, cx);
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

        if cx.global::<UserState>().current_user.is_none() {
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
            .children(modal_layer)
            .child(div().absolute().top_8().children(notification_layer))
    }
}
