use components::{indicator::Indicator, Sizable};
use gpui::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use std::{cmp::Reverse, time::Duration};

use super::Block;
use crate::{
    get_client,
    states::{account::AccountState, room::Room},
};

struct RoomList {
    rooms: Vec<Room>,
}

impl RoomList {
    pub fn new(raw_events: Vec<Event>, cx: &mut ViewContext<'_, Self>) -> Self {
        let rooms: Vec<Room> = raw_events
            .into_iter()
            .map(|event| Room::new(event, cx))
            .collect();

        Self { rooms }
    }
}

impl Render for RoomList {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_1().children(self.rooms.clone())
    }
}

pub struct Rooms {
    rooms: Model<Option<View<RoomList>>>,
    focus_handle: FocusHandle,
}

impl Rooms {
    pub fn view(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::new)
    }

    fn new(cx: &mut ViewContext<Self>) -> Self {
        let rooms = cx.new_model(|_| None);
        let async_rooms = rooms.clone();

        if let Some(public_key) = cx.global::<AccountState>().in_use {
            let client = get_client();
            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(public_key);

            let mut async_cx = cx.to_async();

            cx.foreground_executor()
                .spawn(async move {
                    let events = async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Ok(events) = client.database().query(vec![filter]).await {
                                events
                                    .into_iter()
                                    .sorted_by_key(|ev| Reverse(ev.created_at))
                                    .filter(|ev| ev.pubkey != public_key)
                                    .unique_by(|ev| ev.pubkey)
                                    .collect::<Vec<_>>()
                            } else {
                                Vec::new()
                            }
                        })
                        .await;

                    // Get all public keys
                    let public_keys: Vec<PublicKey> =
                        events.iter().map(|event| event.pubkey).collect();

                    // Calculate total public keys
                    let total = public_keys.len();

                    // Create subscription for metadata events
                    let filter = Filter::new()
                        .kind(Kind::Metadata)
                        .authors(public_keys)
                        .limit(total);

                    let opts = SubscribeAutoCloseOptions::default()
                        .filter(FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(2)));

                    async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Err(e) = client.subscribe(vec![filter], Some(opts)).await {
                                println!("Error: {}", e);
                            }
                        })
                        .await;

                    let view = async_cx.new_view(|cx| RoomList::new(events, cx)).unwrap();

                    _ = async_cx.update_model(&async_rooms, |a, b| {
                        *a = Some(view);
                        b.notify();
                    });
                })
                .detach();
        }

        Self {
            rooms,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Block for Rooms {
    fn title() -> &'static str {
        "Rooms"
    }

    fn new_view(cx: &mut WindowContext) -> View<impl FocusableView> {
        Self::view(cx)
    }

    fn zoomable() -> bool {
        false
    }
}

impl FocusableView for Rooms {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Rooms {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div();

        if let Some(room_list) = self.rooms.read(cx).as_ref() {
            content = content
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .child(room_list.clone());
        } else {
            content = content
                .w_full()
                .flex()
                .justify_center()
                .child(Indicator::new().small())
        }

        div().child(content)
    }
}
