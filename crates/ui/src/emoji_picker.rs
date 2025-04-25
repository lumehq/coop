use std::rc::Rc;

use gpui::{
    div, px, App, AppContext, Element, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window,
};

use crate::{
    button::{Button, ButtonVariants},
    popover::{Popover, PopoverContent},
    theme::{scale::ColorScaleStep, ActiveTheme},
    Icon, IconName,
};

#[derive(IntoElement)]
pub struct EmojiPicker {
    emojis: Rc<Vec<SharedString>>,
}

impl EmojiPicker {
    pub fn new() -> Self {
        let mut emojis: Vec<SharedString> = vec![];

        emojis.extend(
            emojis::Group::SmileysAndEmotion
                .emojis()
                .map(|e| SharedString::from(e.as_str()))
                .collect::<Vec<SharedString>>(),
        );

        emojis.extend(
            emojis::Group::Symbols
                .emojis()
                .map(|e| SharedString::from(e.as_str()))
                .collect::<Vec<SharedString>>(),
        );

        Self {
            emojis: emojis.into(),
        }
    }
}

impl Default for EmojiPicker {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for EmojiPicker {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Popover::new("emoji-picker")
            .anchor(gpui::Corner::BottomLeft)
            .trigger(
                Button::new("emoji-btn")
                    .icon(Icon::new(IconName::EmojiFill))
                    .ghost(),
            )
            .content(move |window, cx| {
                let emojis = self.emojis.clone();

                cx.new(|cx| {
                    PopoverContent::new(window, cx, move |_window, cx| {
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .children(emojis.iter().map(|e| {
                                div()
                                    .flex_auto()
                                    .size_10()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(cx.theme().radius))
                                    .hover(|this| {
                                        this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE))
                                    })
                                    .child(e.clone())
                            }))
                            .into_any()
                    })
                    .scrollable()
                    .max_h(px(300.))
                    .max_w(px(300.))
                })
            })
    }
}
