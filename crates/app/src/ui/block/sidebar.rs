use components::{theme::ActiveTheme, StyledExt};
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::time::Duration;

use super::Block;
use crate::{state::get_client, utils::ago};

#[derive(Clone, IntoElement)]
struct Room {
    #[allow(dead_code)]
    public_key: PublicKey,
    message_at: Timestamp,
    metadata: Model<Option<Metadata>>,
}

impl Room {
    pub fn new(public_key: PublicKey, created_at: Timestamp, cx: &mut WindowContext) -> Self {
        let metadata = cx.new_model(|_| None);
        let async_metadata = metadata.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client().await;
                let metadata = client
                    .fetch_metadata(public_key, Some(Duration::from_secs(2)))
                    .await
                    .unwrap();

                async_metadata
                    .update(&mut async_cx, |a, b| {
                        *a = Some(metadata);
                        b.notify()
                    })
                    .unwrap();
            })
            .detach();

        Self {
            public_key,
            metadata,
            message_at: created_at,
        }
    }
}

impl RenderOnce for Room {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let ago = ago(self.message_at.as_u64());
        let metadata = match self.metadata.read(cx) {
            Some(metadata) => div()
                .flex()
                .gap_2()
                .when_some(metadata.picture.clone(), |parent, picture| {
                    parent.child(
                        img(picture)
                            .size_6()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .when_some(metadata.display_name.clone(), |parent, display_name| {
                    parent.child(display_name).font_medium()
                }),
            None => div()
                .flex()
                .gap_2()
                .child(div().size_6().rounded_full().bg(cx.theme().muted))
                .child("Unnamed"),
        };

        div()
            .flex()
            .justify_between()
            .items_center()
            .px_2()
            .text_sm()
            .child(metadata)
            .child(ago)
    }
}

struct Rooms {
    rooms: Vec<Room>,
}

impl Rooms {
    pub fn new(items: Vec<UnsignedEvent>, cx: &mut ViewContext<'_, Self>) -> Self {
        let rooms: Vec<Room> = items
            .iter()
            .map(|item| Room::new(item.pubkey, item.created_at, cx))
            .collect();

        Self { rooms }
    }
}

impl Render for Rooms {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_2().children(self.rooms.clone())
    }
}

pub struct Sidebar {
    rooms: Model<Option<View<Rooms>>>,
    focus_handle: FocusHandle,
}

impl Sidebar {
    pub fn view(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::new)
    }

    fn new(cx: &mut ViewContext<Self>) -> Self {
        let rooms = cx.new_model(|_| None);
        let async_rooms = rooms.clone();

        let mut async_cx = cx.to_async();

        Self {
            rooms,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Block for Sidebar {
    fn title() -> &'static str {
        "Sidebar"
    }

    fn new_view(cx: &mut WindowContext) -> View<impl FocusableView> {
        Self::view(cx)
    }

    fn zoomable() -> bool {
        false
    }
}

impl FocusableView for Sidebar {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div();

        if let Some(rooms) = self.rooms.read(cx).as_ref() {
            content = content.child(rooms.clone());
        }

        div().pt_4().child(content)
    }
}
