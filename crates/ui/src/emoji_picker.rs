use std::sync::OnceLock;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, AppContext, Corner, Element, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use theme::ActiveTheme;

use crate::button::{Button, ButtonVariants};
use crate::input::InputState;
use crate::popover::{Popover, PopoverContent};
use crate::{Icon, Sizable, Size};

static EMOJIS: OnceLock<Vec<SharedString>> = OnceLock::new();

fn get_emojis() -> &'static Vec<SharedString> {
    EMOJIS.get_or_init(|| {
        let mut emojis: Vec<SharedString> = vec![];

        emojis.extend(
            emojis::Group::SmileysAndEmotion
                .emojis()
                .map(|e| SharedString::from(e.as_str()))
                .collect::<Vec<SharedString>>(),
        );

        emojis
    })
}

#[derive(IntoElement)]
pub struct EmojiPicker {
    icon: Option<Icon>,
    size: Size,
    anchor: Option<Corner>,
    target_input: WeakEntity<InputState>,
}

impl EmojiPicker {
    pub fn new(target_input: WeakEntity<InputState>) -> Self {
        Self {
            target_input,
            size: Size::default(),
            anchor: None,
            icon: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn anchor(mut self, corner: Corner) -> Self {
        self.anchor = Some(corner);
        self
    }
}

impl Sizable for EmojiPicker {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for EmojiPicker {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Popover::new("emoji-picker")
            .map(|this| {
                if let Some(corner) = self.anchor {
                    this.anchor(corner)
                } else {
                    this.anchor(gpui::Corner::BottomLeft)
                }
            })
            .trigger(
                Button::new("emoji-trigger")
                    .when_some(self.icon, |this, icon| this.icon(icon))
                    .ghost()
                    .with_size(self.size),
            )
            .content(move |window, cx| {
                let input = self.target_input.clone();

                cx.new(|cx| {
                    PopoverContent::new(window, cx, move |_window, cx| {
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .children(get_emojis().iter().map(|e| {
                                div()
                                    .id(e.clone())
                                    .flex_auto()
                                    .size_10()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(cx.theme().radius)
                                    .child(e.clone())
                                    .hover(|this| this.bg(cx.theme().ghost_element_hover))
                                    .on_click({
                                        let item = e.clone();
                                        let input = input.upgrade();

                                        move |_, window, cx| {
                                            if let Some(input) = input.as_ref() {
                                                input.update(cx, |this, cx| {
                                                    let current = this.value();
                                                    let new_text = if current.is_empty() {
                                                        format!("{item}")
                                                    } else if current.ends_with(" ") {
                                                        format!("{current}{item}")
                                                    } else {
                                                        format!("{current} {item}")
                                                    };
                                                    this.set_value(new_text, window, cx);
                                                });
                                            }
                                        }
                                    })
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
