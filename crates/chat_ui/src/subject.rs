use gpui::{
    div, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{v_flex, ActiveTheme, Sizable};

pub fn init(subject: Option<String>, window: &mut Window, cx: &mut App) -> Entity<Subject> {
    cx.new(|cx| Subject::new(subject, window, cx))
}

pub struct Subject {
    input: Entity<InputState>,
}

impl Subject {
    pub fn new(subject: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Plan for holiday"));

        if let Some(value) = subject {
            input.update(cx, |this, cx| {
                this.set_value(value, window, cx);
            });
        };

        Self { input }
    }

    pub fn new_subject(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }
}

impl Render for Subject {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from("Subject:")),
                    )
                    .child(Input::new(&self.input).small()),
            )
            .child(
                div()
                    .text_xs()
                    .italic()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(
                        "Subject will be updated when you send a new message.",
                    )),
            )
    }
}
