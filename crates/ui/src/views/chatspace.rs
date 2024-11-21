use gpui::*;

pub struct ChatSpaceView {
    pub text: SharedString,
}

impl ChatSpaceView {
    pub fn new(_cx: &mut ViewContext<'_, Self>) -> ChatSpaceView {
        ChatSpaceView {
            text: "chat".into(),
        }
    }
}

impl Render for ChatSpaceView {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .child(format!("Hello, {}!", &self.text))
    }
}
