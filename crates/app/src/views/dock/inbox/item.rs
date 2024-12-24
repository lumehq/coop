use std::sync::Arc;

use coop_ui::{theme::ActiveTheme, Selectable, StyledExt};
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;

use crate::{
    constants::IMAGE_SERVICE,
    get_client,
    states::{chat::Room, metadata::MetadataRegistry, signal::SignalRegistry},
    utils::{ago, show_npub},
    views::app::AddPanel,
};

#[derive(IntoElement)]
struct Item {
    id: ElementId,
    room: Arc<Room>,
    metadata: Option<Metadata>,
    // Interactive
    base: Div,
    selected: bool,
}

impl Item {
    pub fn new(room: Arc<Room>, metadata: Option<Metadata>) -> Self {
        let id = SharedString::from(room.owner.to_hex()).into();

        Self {
            id,
            room,
            metadata,
            base: div(),
            selected: false,
        }
    }
}

impl Selectable for Item {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn element_id(&self) -> &gpui::ElementId {
        &self.id
    }
}

impl InteractiveElement for Item {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Item {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let ago = ago(self.room.last_seen.as_u64());
        let fallback_name = show_npub(self.room.owner, 16);

        let mut content = div()
            .font_medium()
            .text_color(cx.theme().sidebar_accent_foreground);

        if let Some(metadata) = self.metadata.clone() {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .map(|this| {
                    if let Some(picture) = metadata.picture {
                        this.flex_shrink_0().child(
                            img(format!(
                                "{}/?url={}&w=100&h=100&n=-1",
                                IMAGE_SERVICE, picture
                            ))
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
                    if let Some(display_name) = metadata.display_name {
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

        self.base
            .id(self.id)
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
            .on_click(move |_, cx| {
                cx.dispatch_action(Box::new(AddPanel {
                    room: self.room.clone(),
                    position: coop_ui::dock::DockPlacement::Center,
                }))
            })
    }
}

pub struct InboxItem {
    room: Arc<Room>,
    metadata: Model<Option<Metadata>>,
    pub(crate) sender: PublicKey,
}

impl InboxItem {
    pub fn new(event: Event, cx: &mut ViewContext<'_, Self>) -> Self {
        let sender = event.pubkey;
        let last_seen = event.created_at;

        // Get all members from event's tag
        let mut members: Vec<PublicKey> = event.tags.public_keys().copied().collect();
        // Add sender to members
        members.insert(0, sender);

        // Get title from event's tag
        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
            tag.content().map(|s| s.to_string())
        } else {
            // TODO: create random name?
            None
        };

        let metadata = cx.new_model(|_| None);

        // Request metadata
        _ = cx.global::<SignalRegistry>().tx.send(sender);

        // Reload when received metadata
        cx.observe_global::<MetadataRegistry>(|chat, cx| {
            chat.load_metadata(cx);
        })
        .detach();

        let room = Arc::new(Room {
            title,
            members,
            last_seen,
            owner: sender,
        });

        Self {
            room,
            sender,
            metadata,
        }
    }

    pub fn load_metadata(&mut self, cx: &mut ViewContext<Self>) {
        let public_key = self.sender;
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

    fn render_item(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let metadata = self.metadata.read(cx).clone();
        let room = self.room.clone();

        Item::new(room, metadata)
    }
}

impl Render for InboxItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div().child(self.render_item(cx))
    }
}
