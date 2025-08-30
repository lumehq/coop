use gpui::{
    div, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window,
};
use i18n::t;
use theme::ActiveTheme;
use ui::input::{InputState, TextInput};
use ui::{v_flex, Sizable};

pub fn init(subject: Option<String>, window: &mut Window, cx: &mut App) -> Entity<Subject> {
    Subject::new(subject, window, cx)
}

pub struct Subject {
    input: Entity<InputState>,
}

impl Subject {
    pub fn new(subject: Option<String>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let input = cx.new(|cx| {
            let mut this = InputState::new(window, cx).placeholder(t!("subject.placeholder"));
            if let Some(text) = subject.as_ref() {
                this.set_value(text, window, cx);
            }
            this
        });

        cx.new(|_| Self { input })
    }

    pub fn new_subject(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }
}

impl Render for Subject {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(SharedString::new(t!("subject.title"))),
            )
            .child(TextInput::new(&self.input).small())
            .child(
                div()
                    .text_xs()
                    .italic()
                    .text_color(cx.theme().text_placeholder)
                    .child(SharedString::new(t!("subject.help_text"))),
            )
    }
}
