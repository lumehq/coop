use gpui::*;
use nostr_sdk::prelude::*;

pub struct ChatRegistry {
    pub new_messages: Vec<Event>,
    pub reload: bool,
    pub is_initialized: bool,
}

impl Global for ChatRegistry {}

impl ChatRegistry {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());
    }

    pub fn set_init(&mut self) {
        self.is_initialized = true;
    }

    pub fn set_reload(&mut self) {
        self.reload = true;
    }

    pub fn push(&mut self, event: Event) {
        self.new_messages.push(event);
    }

    fn new() -> Self {
        Self {
            new_messages: Vec::new(),
            reload: false,
            is_initialized: false,
        }
    }
}
