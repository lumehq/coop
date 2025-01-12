use crate::states::chat::room::Member;
use gpui::{
    div, img, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    WindowContext,
};
use ui::{theme::ActiveTheme, StyledExt};

#[derive(Clone, Debug, IntoElement)]
pub struct Message {
    member: Member,
    content: SharedString,
    ago: SharedString,
}

impl Message {
    pub fn new(member: Member, content: SharedString, ago: SharedString) -> Self {
        Self {
            member,
            content,
            ago,
        }
    }
}

impl RenderOnce for Message {
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
            .child(
                img(self.member.avatar())
                    .size_8()
                    .rounded_full()
                    .flex_shrink_0(),
            )
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
                            .child(div().font_semibold().child(self.member.name()))
                            .child(
                                div()
                                    .child(self.ago)
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
