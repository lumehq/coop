use ::client::NostrClient;
use components::{
    input::{InputEvent, TextInput},
    label::Label,
};
use gpui::*;
use nostr_sdk::prelude::*;

use crate::state::AppState;

pub struct SetupView {
    input: View<TextInput>,
}

impl SetupView {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> SetupView {
        let input = cx.new_view(|cx| {
            let mut input = TextInput::new(cx);
            input.set_size(components::Size::Medium, cx);
            input
        });

        cx.subscribe(&input, move |_, text_input, input_event, cx| {
            if let InputEvent::PressEnter = input_event {
                let content = text_input.read(cx).text().to_string();

                if let Ok(keys) = Keys::parse(content) {
                    let public_key = keys.public_key();

                    if cx.global::<NostrClient>().add_account(keys).is_ok() {
                        cx.global_mut::<AppState>().accounts.insert(public_key);
                        cx.notify();
                    }
                };
            }
        })
        .detach();

        SetupView { input }
    }
}

impl Render for SetupView {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_1_3()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new("Private Key").text_sm())
            .child(self.input.clone())
    }
}
