use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::sync::Arc;
use ui::{theme::ActiveTheme, StyledExt};

use crate::{
    constants::IMAGE_SERVICE,
    get_client,
    states::{chat::Room, metadata::MetadataRegistry, signal::SignalRegistry},
    utils::{ago, get_room_id, show_npub},
    views::app::{AddPanel, PanelKind},
};

pub struct InboxListItem {
    id: SharedString,
    event: Event,
    metadata: Model<Option<Metadata>>,
}

impl InboxListItem {
    pub fn new(event: Event, cx: &mut ViewContext<'_, Self>) -> Self {
        let pubkeys: Vec<PublicKey> = event.tags.public_keys().copied().collect();
        let id = get_room_id(&event.pubkey, &pubkeys).into();
        let metadata = cx.new_model(|_| None);

        // Reload when received metadata
        cx.observe_global::<MetadataRegistry>(|chat, cx| {
            chat.load_metadata(cx);
        })
        .detach();

        Self {
            id,
            event,
            metadata,
        }
    }

    pub fn request_metadata(&mut self, cx: &mut ViewContext<Self>) {
        _ = cx.global::<SignalRegistry>().tx.send(self.event.pubkey);
    }

    pub fn load_metadata(&mut self, cx: &mut ViewContext<Self>) {
        let public_key = self.event.pubkey;
        let async_metadata = self.metadata.clone();
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
                };
            })
            .detach();
    }

    pub fn id(&self) -> String {
        self.id.clone().into()
    }

    pub fn action(&self, cx: &mut WindowContext<'_>) {
        let room = Arc::new(Room::new(&self.event, cx));

        cx.dispatch_action(Box::new(AddPanel {
            panel: PanelKind::Room(room),
            position: ui::dock::DockPlacement::Center,
        }))
    }
}

impl Render for InboxListItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let ago = ago(self.event.created_at.as_u64());
        let fallback_name = show_npub(self.event.pubkey, 16);

        let mut content = div()
            .font_medium()
            .text_color(cx.theme().sidebar_accent_foreground);

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
                        this.child(fallback_name)
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
                .child(fallback_name)
        }

        div()
            .id(self.id.clone())
            .h_8()
            .px_1()
            .flex()
            .items_center()
            .justify_between()
            .text_xs()
            .rounded_md()
            .hover(|this| {
                this.bg(cx.theme().sidebar_accent)
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
            .child(content)
            .child(
                div()
                    .child(ago)
                    .text_color(cx.theme().sidebar_accent_foreground.opacity(0.7)),
            )
            .on_click(cx.listener(|this, _, cx| {
                this.action(cx);
            }))
    }
}
