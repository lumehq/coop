use gpui::*;
use nostr_sdk::prelude::*;

#[derive(Clone)]
pub struct RoomLastMessage {
    pub content: Option<String>,
    pub time: Timestamp,
}

#[derive(Clone, IntoElement)]
pub struct Room {
    members: Vec<PublicKey>,
    last_message: Option<RoomLastMessage>,
}

impl Room {
    pub fn new(members: Vec<PublicKey>, last_message: Option<RoomLastMessage>) -> Self {
        Self {
            members,
            last_message,
        }
    }
}

impl RenderOnce for Room {
    fn render(self, _cx: &mut WindowContext) -> impl IntoElement {
        div().child("TODO")
    }
}

pub struct Rooms {
    pub rooms: Vec<Room>,
}

impl Global for Rooms {}
