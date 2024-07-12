use freya::prelude::*;

pub mod user;
pub mod chat;

#[derive(Clone, PartialEq)]
pub enum Direction {
	VERTICAL,
	HORIZONTAL,
}

#[component]
pub fn Divider(background: String, direction: Direction) -> Element {
	match direction {
		Direction::VERTICAL => rsx!(
			rect {
				width: "1",
				height: "100%",
				background: background,
			}
		),
		Direction::HORIZONTAL => rsx!(
			rect {
				width: "100%",
				height: "1",
				background: background,
			}
		)
	}
}
