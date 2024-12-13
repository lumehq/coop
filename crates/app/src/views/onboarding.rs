use async_utility::task::spawn;
use coop_ui::{
    input::{InputEvent, TextInput},
    label::Label,
};
use gpui::*;
use keyring::Entry;
use nostr_sdk::prelude::*;

use crate::{constants::KEYRING_SERVICE, get_client, states::account::AccountRegistry};

pub struct Onboarding {
    input: View<TextInput>,
}

impl Onboarding {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let input = cx.new_view(|cx| {
            let mut input = TextInput::new(cx);
            input.set_size(coop_ui::Size::Medium, cx);
            input
        });

        cx.subscribe(&input, move |_, text_input, input_event, cx| {
            if let InputEvent::PressEnter = input_event {
                let content = text_input.read(cx).text().to_string();
                _ = Self::save_keys(&content, cx);
            }
        })
        .detach();

        Self { input }
    }

    fn save_keys(content: &str, cx: &mut ViewContext<Self>) -> anyhow::Result<(), anyhow::Error> {
        let keys = Keys::parse(content)?;

        let public_key = keys.public_key();
        let bech32 = public_key.to_bech32().unwrap();
        let secret = keys.secret_key().to_secret_hex();

        let entry = Entry::new(KEYRING_SERVICE, &bech32).unwrap();

        // Save secret key to keyring
        entry.set_password(&secret)?;

        // Update signer
        spawn(async move {
            get_client().set_signer(keys).await;
        });

        // Update view
        cx.update_global(|state: &mut AccountRegistry, cx| {
            state.set_user(Some(public_key));
            cx.notify();
        });

        Ok(())
    }
}

impl Render for Onboarding {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size_1_3()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new("Private Key").text_sm())
                    .child(self.input.clone()),
            )
    }
}
