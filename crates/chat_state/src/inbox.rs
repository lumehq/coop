use gpui::{Context, Entity};

use crate::room::Room;

pub struct Inbox {
    pub rooms: Vec<Entity<Room>>,
    pub is_loading: bool,
}

impl Inbox {
    pub fn new() -> Self {
        Self {
            rooms: vec![],
            is_loading: true,
        }
    }

    pub fn ids(&self, cx: &Context<Self>) -> Vec<u64> {
        self.rooms.iter().map(|room| room.read(cx).id).collect()
    }
}

impl Default for Inbox {
    fn default() -> Self {
        Self::new()
    }
}
