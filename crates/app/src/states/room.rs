use components::theme::ActiveTheme;
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;

use crate::get_client;

#[derive(Clone)]
#[allow(dead_code)]
struct RoomLastMessage {
    content: Option<String>,
    time: Timestamp,
}

#[derive(Clone, IntoElement)]
pub struct Room {
    #[allow(dead_code)]
    public_key: PublicKey,
    metadata: Model<Option<Metadata>>,
    #[allow(dead_code)]
    last_message: RoomLastMessage,
}

impl Room {
    pub fn new(event: Event, cx: &mut WindowContext) -> Self {
        let public_key = event.pubkey;

        let last_message = RoomLastMessage {
            content: Some(event.content),
            time: event.created_at,
        };

        let metadata = cx.new_model(|_| None);
        let async_metadata = metadata.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();

                let query = async_cx
                    .background_executor()
                    .spawn(async move { client.database().metadata(public_key).await })
                    .await;

                if let Ok(metadata) = query {
                    _ = async_cx.update_model(&async_metadata, |a, b| {
                        *a = metadata;
                        b.notify();
                    });
                }
            })
            .detach();

        Self {
            public_key,
            metadata,
            last_message,
        }
    }
}

impl RenderOnce for Room {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let mut content = div();

        if let Some(metadata) = self.metadata.read(cx).as_ref() {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .when_some(metadata.picture.clone(), |div, picture| {
                    div.flex_shrink_0().child(
                        img(picture)
                            .size_6()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .when_some(metadata.display_name.clone(), |div, display_name| {
                    div.child(display_name)
                })
        } else {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .child(
                    div()
                        .flex_shrink_0()
                        .size_6()
                        .rounded_full()
                        .bg(cx.theme().muted),
                )
                .child("Anon")
        }

        div().child(content)
    }
}
