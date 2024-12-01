use gpui::*;
use nostr_sdk::prelude::*;

#[derive(Clone)]
pub struct UserState {
    pub current_user: Option<PublicKey>,
}

impl Global for UserState {}

impl UserState {
    pub fn set_global(cx: &mut AppContext) {
        cx.set_global(Self::new());
    }

    fn new() -> Self {
        Self { current_user: None }
    }
}
