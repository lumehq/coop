use gpui::{
    div, img, prelude::FluentBuilder, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, WindowContext,
};
use nostr_sdk::prelude::*;
use ui::{theme::ActiveTheme, StyledExt};

use crate::{
    constants::IMAGE_SERVICE,
    utils::{ago, show_npub},
};

#[derive(Clone, Debug, IntoElement)]
pub struct RoomMessage {
    #[allow(dead_code)]
    author: PublicKey,
    fallback: SharedString,
    metadata: Metadata,
    content: SharedString,
    created_at: SharedString,
}

impl RoomMessage {
    pub fn new(
        author: PublicKey,
        metadata: Metadata,
        content: String,
        created_at: Timestamp,
    ) -> Self {
        let created_at = ago(created_at.as_u64()).into();
        let fallback = show_npub(author, 16).into();

        Self {
            author,
            metadata,
            fallback,
            created_at,
            content: content.into(),
        }
    }
}

impl RenderOnce for RoomMessage {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        div()
            .flex()
            .gap_3()
            .w_full()
            .p_2()
            .border_l_2()
            .border_color(cx.theme().background)
            .hover(|this| {
                this.bg(cx.theme().muted)
                    .border_color(cx.theme().primary_active)
                    .text_color(cx.theme().muted_foreground)
            })
            .child(div().flex_shrink_0().map(|this| {
                if let Some(picture) = self.metadata.picture {
                    this.child(
                        img(format!(
                            "{}/?url={}&w=100&h=100&fit=cover&mask=circle&n=-1",
                            IMAGE_SERVICE, picture
                        ))
                        .size_8(),
                    )
                } else {
                    this.child(img("brand/avatar.png").size_8().rounded_full())
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_initial()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .text_xs()
                            .child(div().font_semibold().map(|this| {
                                if let Some(display_name) = self.metadata.display_name {
                                    this.child(display_name)
                                } else {
                                    this.child(self.fallback)
                                }
                            }))
                            .child(
                                div()
                                    .child(self.created_at)
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(self.content),
                    ),
            )
    }
}
