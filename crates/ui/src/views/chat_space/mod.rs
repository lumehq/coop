use bottom_bar::BottomBar;
use components::{
    resizable::{h_resizable, resizable_panel, ResizablePanelGroup},
    theme::ActiveTheme,
};
use gpui::*;

pub mod bottom_bar;

pub struct ChatSpace {
    layout: View<ResizablePanelGroup>,
}

impl ChatSpace {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let bottom_bar = cx.new_view(BottomBar::new);
        // TODO: add chat list view

        let layout = cx.new_view(|cx| {
            h_resizable(cx)
                .child(
                    resizable_panel().size(px(260.)).content(move |cx| {
                        div()
                            .size_full()
                            .bg(cx.theme().secondary)
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w_full()
                                    .child("Chat List"),
                            )
                            .child(bottom_bar.clone())
                            .into_any_element()
                    }),
                    cx,
                )
                .child(
                    resizable_panel().content(|_| div().child("Content").into_any_element()),
                    cx,
                )
        });

        Self { layout }
    }
}

impl Render for ChatSpace {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div().relative().size_full().child(self.layout.clone())
    }
}
