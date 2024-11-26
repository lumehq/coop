use bottom_bar::BottomBar;
use components::{
    resizable::{h_resizable, resizable_panel, ResizablePanelGroup},
    theme::ActiveTheme,
};
use gpui::*;
use navigation::Navigation;

pub mod bottom_bar;
pub mod navigation;

pub struct ChatSpace {
    layout: View<ResizablePanelGroup>,
}

impl ChatSpace {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let navigation = cx.new_view(Navigation::new);

        let layout = cx.new_view(|cx| {
            h_resizable(cx)
                .child(
                    resizable_panel().size(px(260.)).content(move |cx| {
                        div()
                            .size_full()
                            .bg(cx.theme().side_bar_background)
                            .text_color(cx.theme().side_bar_foreground)
                            .flex()
                            .flex_col()
                            .child(navigation.clone())
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
