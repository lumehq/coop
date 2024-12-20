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

        cx.subscribe(&input, move |form, _, input_event, cx| {
            if let InputEvent::PressEnter = input_event {
                form.send_message(cx);
            }
        })
        .detach();

        Self { to, input }
    }

    fn send_message(&mut self, cx: &mut ViewContext<Self>) {
        let send_to = self.to;
        let content = self.input.read(cx).text().to_string();
        let content_clone = content.clone();

        let async_input = self.input.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();

                async_cx
                    .background_executor()
                    .spawn(async move {
                        let signer = client.signer().await.unwrap();
                        let public_key = signer.get_public_key().await.unwrap();

                        // Send message to all members
                        if client
                            .send_private_msg(send_to, content, vec![])
                            .await
                            .is_ok()
                        {
                            // Send a copy to yourself
                            _ = client
                                .send_private_msg(
                                    public_key,
                                    content_clone,
                                    vec![Tag::public_key(send_to)],
                                )
                                .await;
                        }
                    })
                    .await;

                _ = async_cx.update_view(&async_input, |input, cx| {
                    input.set_text("", cx);
                });
            })
            .detach();
    }
}

impl Render for Form {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .flex_shrink_0()
            .w_full()
            .h_12()
            .border_t_1()
            .border_color(cx.theme().border.opacity(0.7))
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
