use components::{theme::ActiveTheme, Collapsible, Selectable, StyledExt};
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use serde::Deserialize;

use crate::{
    utils::{ago, show_npub},
    views::app::AddPanel,
};

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ChatDelegate {
    title: Option<String>,
    public_key: PublicKey,
    metadata: Option<Metadata>,
    last_seen: Timestamp,
}

impl ChatDelegate {
    pub fn new(
        title: Option<String>,
        public_key: PublicKey,
        metadata: Option<Metadata>,
        last_seen: Timestamp,
    ) -> Self {
        Self {
            title,
            public_key,
            metadata,
            last_seen,
        }
    }
}

#[derive(IntoElement)]
pub struct Chat {
    id: ElementId,
    pub item: ChatDelegate,
    // Interactive
    base: Div,
    selected: bool,
    is_collapsed: bool,
}

impl Chat {
    pub fn new(item: ChatDelegate) -> Self {
        let id = SharedString::from(item.public_key.to_hex()).into();

        Self {
            id,
            item,
            base: div(),
            selected: false,
            is_collapsed: false,
        }
    }
}

impl Selectable for Chat {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn element_id(&self) -> &gpui::ElementId {
        &self.id
    }
}

impl Collapsible for Chat {
    fn is_collapsed(&self) -> bool {
        self.is_collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.is_collapsed = collapsed;
        self
    }
}

impl InteractiveElement for Chat {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Chat {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let ago = ago(self.item.last_seen.as_u64());

        let mut content = div()
            .font_medium()
            .text_color(cx.theme().sidebar_accent_foreground);

        if let Some(metadata) = self.item.metadata.clone() {
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
                            .child(div().size_6().rounded_full().bg(cx.theme().muted))
                    }
                })
                .map(|this| {
                    if let Some(display_name) = metadata.display_name {
                        this.child(display_name)
                    } else if let Ok(npub) = show_npub(self.item.public_key, 16) {
                        this.child(npub)
                    } else {
                        this.child("Anon")
                    }
                })
        } else {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .size_6()
                        .rounded_full()
                        .bg(cx.theme().muted),
                )
                .child("Anon")
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
                    title: self.item.title.clone(),
                    receiver: self.item.public_key,
                }))
            })
    }
}
