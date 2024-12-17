use coop_ui::{
    button::{Button, ButtonVariants},
    input::{InputEvent, TextInput},
    theme::ActiveTheme,
    Icon, IconName,
};
use gpui::*;
use nostr_sdk::prelude::*;

use crate::get_client;

pub struct Form {
    to: PublicKey,
    input: View<TextInput>,
}

impl Form {
    pub fn new(to: PublicKey, cx: &mut ViewContext<'_, Self>) -> Self {
        let input = cx.new_view(|cx| {
            TextInput::new(cx)
                .appearance(false)
                .text_size(coop_ui::Size::Small)
                .placeholder("Message...")
                .cleanable()
        });

        cx.subscribe(&input, move |form, text_input, input_event, cx| {
            if let InputEvent::PressEnter = input_event {
                let content = text_input.read(cx).text().to_string();
                // TODO: clean up content

                form.send_message(content, cx);
            }
        })
        .detach();

        Self { to, input }
    }

    fn send_message(&mut self, content: String, cx: &mut ViewContext<Self>) {
        let send_to = self.to;
        let content_clone = content.clone();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();
                let public_key = signer.get_public_key().await.unwrap();

                match client.send_private_msg(send_to, content, vec![]).await {
                    Ok(_) => {
                        // Send a copy to yourself
                        if let Err(_e) = client
                            .send_private_msg(public_key, content_clone, vec![])
                            .await
                        {
                            todo!()
                        }
                    }
                    Err(_) => todo!(),
                }
            })
            .detach();
    }
}

impl Render for Form {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .h_12()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .child(
                Button::new("upload")
                    .icon(Icon::new(IconName::Upload))
                    .ghost(),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .bg(cx.theme().muted)
                    .rounded(px(cx.theme().radius))
                    .px_2()
                    .child(self.input.clone()),
            )
    }
}
