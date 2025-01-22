use crate::states::chat::room::Member;
use gpui::{
    div, img, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    WindowContext,
};
use ui::{
    theme::{scale::ColorScaleStep, ActiveTheme},
    StyledExt,
};

#[derive(Clone, Debug, IntoElement)]
pub struct Message {
    member: Member,
    content: SharedString,
    ago: SharedString,
}

impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        let content = self.content == other.content;
        let member = self.member == other.member;
        let ago = self.ago == other.ago;

        content && member && ago
    }
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
                this.bg(cx.theme().accent.step(cx, ColorScaleStep::ONE))
                    .border_color(cx.theme().accent.step(cx, ColorScaleStep::NINE))
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
                                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN)),
                            ),
                    )
                    .child(div().text_sm().child(self.content)),
            )
    }
}
