use components::{theme::ActiveTheme, Icon, IconName};
use gpui::*;

#[derive(IntoElement)]
struct NavItem {
    text: SharedString,
    icon: Icon,
}

impl NavItem {
    pub fn new(text: SharedString, icon: Icon) -> Self {
        Self { text, icon }
    }
}

impl RenderOnce for NavItem {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        div()
            .hover(|this| {
                this.bg(cx.theme().side_bar_accent)
                    .text_color(cx.theme().side_bar_accent_foreground)
            })
            .rounded_md()
            .flex()
            .items_center()
            .h_7()
            .px_2()
            .gap_2()
            .child(self.icon)
            .child(div().pt(px(2.)).child(self.text))
    }
}

pub struct Navigation {}

impl Navigation {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        Self {}
    }
}

impl Render for Navigation {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div().flex_1().w_full().px_2().child(div().h_11()).child(
            div().flex().flex_col().gap_4().child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child(NavItem::new(
                        "Find".into(),
                        Icon::new(IconName::Search)
                            .path("icons/search.svg")
                            .size_4()
                            .flex_shrink_0()
                            .text_color(cx.theme().foreground),
                    ))
                    .child(NavItem::new(
                        "Messages".into(),
                        Icon::new(IconName::Search)
                            .path("icons/messages.svg")
                            .size_4()
                            .flex_shrink_0()
                            .text_color(cx.theme().foreground),
                    ))
                    .child(NavItem::new(
                        "Notifications".into(),
                        Icon::new(IconName::Search)
                            .path("icons/notifications.svg")
                            .size_4()
                            .flex_shrink_0()
                            .text_color(cx.theme().foreground),
                    ))
                    .child(NavItem::new(
                        "explore".into(),
                        Icon::new(IconName::Search)
                            .path("icons/explore.svg")
                            .size_4()
                            .flex_shrink_0()
                            .text_color(cx.theme().foreground),
                    )),
            ),
        )
    }
}
