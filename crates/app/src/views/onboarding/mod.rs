use common::constants::KEYRING_SERVICE;
use gpui::{div, IntoElement, ParentElement, Render, Styled, View, ViewContext, VisualContext};
use nostr_sdk::prelude::*;
use registry::{app::AppRegistry, contact::Contact};
use state::get_client;
use ui::input::{InputEvent, TextInput};

pub struct Onboarding {
    input: View<TextInput>,
}

impl Onboarding {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let input = cx.new_view(|cx| {
            let mut input = TextInput::new(cx);
            input.set_size(ui::Size::Medium, cx);
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
        let bech32 = public_key.to_bech32()?;
        let secret = keys.secret_key().to_secret_hex();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();
                let task = cx.write_credentials(KEYRING_SERVICE, &bech32, secret.as_bytes());

                async move {
                    if task.await.is_ok() {
                        let query: anyhow::Result<Metadata, anyhow::Error> = async_cx
                            .background_executor()
                            .spawn(async move {
                                // Update signer
                                _ = client.set_signer(keys).await;

                                // Get metadata
                                if let Some(metadata) =
                                    client.database().metadata(public_key).await?
                                {
                                    Ok(metadata)
                                } else {
                                    Ok(Metadata::new())
                                }
                            })
                            .await;

                        if let Ok(metadata) = query {
                            _ = async_cx.update_global::<AppRegistry, _>(|state, cx| {
                                state.set_user(Contact::new(public_key, metadata), cx);
                            });
                        }
                    }
                }
            })
            .detach();

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
                    .child(div().child("Private Key").text_sm())
                    .child(self.input.clone()),
            )
    }
}
