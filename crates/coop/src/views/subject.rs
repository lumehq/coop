use gpui::{
    div, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Window,
};
use i18n::t;
use registry::Registry;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::{ContextModal, Sizable};

pub fn init(
    id: u64,
    subject: Option<String>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Subject> {
    Subject::new(id, subject, window, cx)
}

pub struct Subject {
    id: u64,
    input: Entity<InputState>,
    focus_handle: FocusHandle,
}

impl Subject {
    pub fn new(
        id: u64,
        subject: Option<String>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let input = cx.new(|cx| {
            let mut this = InputState::new(window, cx).placeholder(t!("subject.placeholder"));
            if let Some(text) = subject.clone() {
                this.set_value(text, window, cx);
            }
            this
        });

        cx.new(|cx| Self {
            id,
            input,
            focus_handle: cx.focus_handle(),
        })
    }

    pub fn update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let registry = Registry::global(cx).read(cx);
        let subject = self.input.read(cx).value().clone();

        if let Some(room) = registry.room(&self.id, cx) {
            room.update(cx, |this, cx| {
                this.subject = Some(subject);
                cx.notify();
            });
            window.close_modal(cx);
        } else {
            window.push_notification(SharedString::new(t!("subject.room_not_found")), cx);
        }
    }
}

impl Render for Subject {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .px_3()
            .pb_3()
            .child(
                div()
                    .flex()
                    .flex_col()
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
                    ),
            )
            .child(
                Button::new("submit")
                    .label(t!("common.change"))
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| this.update(window, cx))),
            )
    }
}
