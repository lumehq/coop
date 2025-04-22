use chats::ChatRegistry;
use gpui::{
    div, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window,
};
use ui::{
    button::{Button, ButtonVariants},
    input::TextInput,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Size,
};

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
    input: Entity<TextInput>,
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
            let mut this = TextInput::new(window, cx).text_size(Size::Small);
            if let Some(text) = subject.clone() {
                this.set_text(text, window, cx);
            } else {
                this.set_placeholder("prepare for holidays...");
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
        let registry = ChatRegistry::global(cx).read(cx);
        let subject = self.input.read(cx).text();

        if subject.is_empty() {
            window.push_notification("Subject cannot be empty", cx);
            return;
        }

        if let Some(room) = registry.room(&self.id, cx) {
            room.update(cx, |this, cx| {
                this.subject = Some(subject);
                cx.notify();
            });
            window.close_modal(cx);
        } else {
            window.push_notification("Room not found", cx);
        }
    }
}

impl Render for Subject {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const HELP_TEXT: &str = "Subject will be updated when you send a message.";

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                            .child("Subject:"),
                    )
                    .child(self.input.clone())
                    .child(
                        div()
                            .text_xs()
                            .italic()
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::NINE))
                            .child(HELP_TEXT),
                    ),
            )
            .child(
                Button::new("submit")
                    .label("Change")
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| this.update(window, cx))),
            )
    }
}
