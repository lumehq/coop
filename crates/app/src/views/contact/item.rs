use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use ui::theme::ActiveTheme;

use crate::{
    constants::IMAGE_SERVICE,
    get_client,
    states::{metadata::MetadataRegistry, signal::SignalRegistry},
    utils::show_npub,
};

pub struct ContactListItem {
    public_key: PublicKey,
    metadata: Model<Option<Metadata>>,
}

impl ContactListItem {
    pub fn new(public_key: PublicKey, cx: &mut ViewContext<'_, Self>) -> Self {
        let metadata = cx.new_model(|_| None);

        // Request metadata
        _ = cx.global::<SignalRegistry>().tx.send(public_key);

        // Reload when received metadata
        cx.observe_global::<MetadataRegistry>(|item, cx| {
            item.load_metadata(cx);
        })
        .detach();

        Self {
            public_key,
            metadata,
        }
    }

    pub fn load_metadata(&mut self, cx: &mut ViewContext<Self>) {
        let public_key = self.public_key;
        let async_metadata = self.metadata.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();

                async move {
                    let query = async_cx
                        .background_executor()
                        .spawn(async move { client.database().metadata(public_key).await })
                        .await;

                    if let Ok(metadata) = query {
                        _ = async_cx.update_model(&async_metadata, |a, b| {
                            *a = metadata;
                            b.notify();
                        });
                    };
                }
            })
            .detach();
    }
}

impl Render for ContactListItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let fallback = show_npub(self.public_key, 16);
        let mut content = div()
            .w_full()
            .h_10()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .text_sm();

        if let Some(metadata) = self.metadata.read(cx).as_ref() {
            content = content
                .map(|this| {
                    if let Some(picture) = metadata.picture.clone() {
                        this.flex_shrink_0().child(
                            img(format!(
                                "{}/?url={}&w=72&h=72&fit=cover&mask=circle&n=-1",
                                IMAGE_SERVICE, picture
                            ))
                            .size_8(),
                        )
                    } else {
                        this.flex_shrink_0()
                            .child(img("brand/avatar.png").size_8().rounded_full())
                    }
                })
                .map(|this| {
                    if let Some(display_name) = metadata.display_name.clone() {
                        this.flex_1().child(display_name)
                    } else {
                        this.flex_1().child(fallback)
                    }
                })
        } else {
            content = content
                .child(img("brand/avatar.png").size_8().rounded_full())
                .child(fallback)
        }

        div()
            .rounded_md()
            .hover(|this| {
                this.bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
            })
            .child(content)
    }
}
