use gpui::{div, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Window};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Settings> {
    Settings::new(window, cx)
}

pub struct Settings {
    //
}

impl Settings {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {})
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child("TODO")
    }
}
