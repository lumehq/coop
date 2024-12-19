use coop_ui::{theme::ActiveTheme, Selectable, StyledExt};
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;

use crate::{
    get_client,
    states::{metadata::MetadataRegistry, signal::SignalRegistry},
    utils::{ago, show_npub},
    views::app::AddPanel,
};

#[derive(IntoElement)]
struct ChatItem {
    id: ElementId,
    public_key: PublicKey,
    metadata: Option<Metadata>,
    last_seen: Timestamp,
    title: Option<String>,
    // Interactive
    base: Div,
    selected: bool,
}

impl ChatItem {
    pub fn new(
        public_key: PublicKey,
        metadata: Option<Metadata>,
        last_seen: Timestamp,
        title: Option<String>,
    ) -> Self {
        let id = SharedString::from(public_key.to_hex()).into();

        Self {
            id,
            public_key,
            metadata,
            last_seen,
            title,
            base: div(),
            selected: false,
        }
    }
}

impl Selectable for ChatItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn element_id(&self) -> &gpui::ElementId {
        &self.id
    }
}

impl InteractiveElement for ChatItem {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for ChatItem {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let ago = ago(self.last_seen.as_u64());
        let fallback_name = show_npub(self.public_key, 16);

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
                            img(picture)
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
                    title: self.title.clone(),
                    from: self.public_key,
                }))
            })
    }
}

pub struct Chat {
    title: Option<String>,
    metadata: Model<Option<Metadata>>,
    last_seen: Timestamp,
    pub(crate) public_key: PublicKey,
}

impl Chat {
    pub fn new(event: Event, cx: &mut ViewContext<'_, Self>) -> Self {
        let public_key = event.pubkey;
        let last_seen = event.created_at;
        let title = if let Some(tag) = event.tags.find(TagKind::Title) {
            tag.content().map(|s| s.to_string())
        } else {
            None
        };

        let metadata = cx.new_model(|_| None);

        // Request metadata
        _ = cx.global::<SignalRegistry>().tx.send(public_key);

        // Reload when received metadata
        cx.observe_global::<MetadataRegistry>(|chat, cx| {
            chat.load_metadata(cx);
        })
        .detach();

        Self {
            public_key,
            last_seen,
            metadata,
            title,
        }
    }

    pub fn load_metadata(&mut self, cx: &mut ViewContext<Self>) {
        let public_key = self.public_key;
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
}

impl Render for Chat {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let metadata = self.metadata.read(cx).clone();

        div().child(ChatItem::new(
            self.public_key,
            metadata,
            self.last_seen,
            self.title.clone(),
        ))
    }
}
