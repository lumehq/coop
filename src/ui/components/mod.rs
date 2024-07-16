use freya::prelude::*;

pub mod chat;
pub mod user;

#[derive(Clone, PartialEq)]
pub enum Direction {
    Vertical,
    Horizontal,
}

#[component]
pub fn Divider(background: String, direction: Direction) -> Element {
    match direction {
        Direction::Vertical => rsx!(rect {
            width: "1",
            height: "100%",
            background: background,
        }),
        Direction::Horizontal => rsx!(rect {
            width: "100%",
            height: "1",
            background: background,
        }),
    }
}
