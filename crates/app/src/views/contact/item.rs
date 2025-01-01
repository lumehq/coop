use coop_ui::StyledExt;
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;

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
        let mut content = div();

        if let Some(metadata) = self.metadata.read(cx).as_ref() {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .map(|this| {
                    if let Some(picture) = metadata.picture.clone() {
                        this.flex_shrink_0().child(
                            img(format!("{}/?url={}&w=72&h=72&n=-1", IMAGE_SERVICE, picture))
                                .size_6()
                                .rounded_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    } else {
                        this.flex_shrink_0()
                            .child(img("brand/avatar.png").size_6().rounded_full())
                    }
                })
                .map(|this| {
                    if let Some(display_name) = metadata.display_name.clone() {
                        this.child(display_name)
                    } else {
                        this.child(fallback)
                    }
                })
        } else {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .child(
                    img("brand/avatar.png")
                        .flex_shrink_0()
                        .size_6()
                        .rounded_full(),
                )
                .child(fallback)
        }

        div()
            .scrollable(
                cx.view().entity_id(),
                coop_ui::scroll::ScrollbarAxis::Vertical,
            )
            .child(content)
    }
}
