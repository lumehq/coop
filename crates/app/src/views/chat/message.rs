use gpui::{
    div, img, px, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window,
};
use registry::contact::Contact;
use ui::{
    theme::{scale::ColorScaleStep, ActiveTheme},
    StyledExt,
};

#[derive(Clone, Debug, IntoElement)]
pub struct Message {
    member: Contact,
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
    pub fn new(member: Contact, content: SharedString, ago: SharedString) -> Self {
        Self {
            member,
            content,
            ago,
        }
    }
}

impl RenderOnce for Message {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .group(&self.ago)
            .relative()
            .flex()
            .gap_3()
            .w_full()
            .p_2()
            .hover(|this| this.bg(cx.theme().accent.step(cx, ColorScaleStep::ONE)))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(2.))
                    .h_full()
                    .bg(cx.theme().transparent)
                    .group_hover(&self.ago, |this| {
                        this.bg(cx.theme().accent.step(cx, ColorScaleStep::NINE))
                    }),
            )
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
