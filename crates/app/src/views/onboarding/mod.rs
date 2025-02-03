use app_state::registry::AppRegistry;
use common::{constants::KEYRING_SERVICE, profile::NostrProfile};
use gpui::{
    div, AppContext, BorrowAppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Window,
};
use nostr_sdk::prelude::*;
use state::get_client;
use ui::{
    input::{InputEvent, TextInput},
    Root,
};

use super::app::AppView;

pub struct Onboarding {
    input: Entity<TextInput>,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_size(ui::Size::Medium, window, cx);
            input
        });

        cx.subscribe_in(
            &input,
            window,
            move |_, text_input, input_event, window, cx| {
                if let InputEvent::PressEnter = input_event {
                    let content = text_input.read(cx).text().to_string();
                    _ = Self::save_keys(&content, window, cx);
                }
            },
        )
        .detach();

        Self { input }
    }

    fn save_keys(
        content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<(), anyhow::Error> {
        let keys = Keys::parse(content)?;
        let public_key = keys.public_key();
        let bech32 = public_key.to_bech32()?;
        let secret = keys.secret_key().to_secret_hex();
        let window_handle = window.window_handle();
        let task = cx.write_credentials(KEYRING_SERVICE, &bech32, secret.as_bytes());

        cx.spawn(|_, mut cx| async move {
            let client = get_client();

            if task.await.is_ok() {
                let (tx, mut rx) = tokio::sync::mpsc::channel::<NostrProfile>(1);

                cx.background_executor()
                    .spawn(async move {
                        // Update signer
                        _ = client.set_signer(keys).await;

                        // Get metadata
                        let metadata = if let Ok(Some(metadata)) =
                            client.database().metadata(public_key).await
                        {
                            metadata
                        } else {
                            Metadata::new()
                        };

                        _ = tx.send(NostrProfile::new(public_key, metadata)).await;
                    })
                    .await;

                while let Some(profile) = rx.recv().await {
                    cx.update_window(window_handle, |_, window, cx| {
                        cx.update_global::<AppRegistry, _>(|this, cx| {
                            this.set_user(Some(profile.clone()));

                            if let Some(root) = this.root() {
                                cx.update_entity(&root, |this: &mut Root, cx| {
                                    this.set_view(
                                        cx.new(|cx| AppView::new(profile, window, cx)).into(),
                                        cx,
                                    );
                                });
                            }
                        });
                    })
                    .unwrap();
                }
            }
        })
        .detach();

        Ok(())
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
