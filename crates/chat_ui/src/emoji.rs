use std::sync::OnceLock;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, AppContext, Corner, Element, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::InputState;
use ui::popover::{Popover, PopoverContent};
use ui::{Icon, Sizable, Size};

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
    target: Option<WeakEntity<InputState>>,
    icon: Option<Icon>,
    anchor: Option<Corner>,
    size: Size,
}

impl EmojiPicker {
    pub fn new() -> Self {
        Self {
            size: Size::default(),
            target: None,
            anchor: None,
            icon: None,
        }
    }

    pub fn target(mut self, target: WeakEntity<InputState>) -> Self {
        self.target = Some(target);
        self
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[allow(dead_code)]
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
        Popover::new("emojis")
            .map(|this| {
                if let Some(corner) = self.anchor {
                    this.anchor(corner)
                } else {
                    this.anchor(gpui::Corner::BottomLeft)
                }
            })
            .trigger(
                Button::new("emojis-trigger")
                    .when_some(self.icon, |this, icon| this.icon(icon))
                    .ghost()
                    .with_size(self.size),
            )
            .content(move |window, cx| {
                let input = self.target.clone();

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
                                        let input = input.clone();

                                        move |_, window, cx| {
                                            if let Some(input) = input.as_ref() {
                                                _ = input.update(cx, |this, cx| {
                                                    let value = this.value();
                                                    let new_text = if value.is_empty() {
                                                        format!("{item}")
                                                    } else if value.ends_with(" ") {
                                                        format!("{value}{item}")
                                                    } else {
                                                        format!("{value} {item}")
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
