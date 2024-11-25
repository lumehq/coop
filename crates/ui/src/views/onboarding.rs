use ::client::NostrClient;
use components::{
    input::{InputEvent, TextInput},
    label::Label,
};
use gpui::*;
use keyring::Entry;
use nostr_sdk::prelude::*;

use crate::{constants::KEYRING_SERVICE, state::AppState};

pub struct Onboarding {
    input: View<TextInput>,
}

impl Onboarding {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let input = cx.new_view(|cx| {
            let mut input = TextInput::new(cx);
            input.set_size(components::Size::Medium, cx);
            input
        });

        cx.subscribe(&input, move |_, text_input, input_event, cx| {
            let mut async_cx = cx.to_async();
            let client = cx.global::<NostrClient>().client;
            let view_id = cx.parent_view_id();

            if let InputEvent::PressEnter = input_event {
                let content = text_input.read(cx).text().to_string();

                if let Ok(keys) = Keys::parse(content) {
                    cx.foreground_executor()
                        .spawn(async move {
                            let public_key = keys.public_key();
                            let secret = keys.secret_key().to_secret_hex();

                            let entry =
                                Entry::new(KEYRING_SERVICE, &public_key.to_bech32().unwrap())
                                    .unwrap();

                            // Store private key to OS Keyring
                            let _ = entry.set_password(&secret);

                            // Update signer
                            client.set_signer(keys).await;

                            // Update view
                            async_cx.update_global(|app_state: &mut AppState, cx| {
                                app_state.signer = Some(public_key);
                                cx.notify(view_id);
                            })
                        })
                        .detach();
                }
            }
        })
        .detach();

        Self { input }
    }
}

impl Render for Onboarding {
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
