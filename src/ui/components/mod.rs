use freya::prelude::*;

use crate::theme::SMOOTHING;

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


#[component]
pub fn Spinner() -> Element {
	let anim = use_animation(|ctx| {
		ctx.auto_start(true);
		ctx.on_finish(OnFinish::Restart);
		ctx.with(AnimNum::new(0.0, 360.0).time(650))
	});

	let degrees = anim.get().read().as_f32();

	rsx!(
		svg {
	        rotate: "{degrees}deg",
	        width: "24",
	        height: "24",
	        svg_content: r#"
	            <svg viewBox="0 0 600 600" xmlns="http://www.w3.org/2000/svg">
	                <circle class="spin" cx="300" cy="300" fill="none"
	                r="250" stroke-width="64" stroke="currentColor"
	                stroke-dasharray="256 1400"
	                stroke-linecap="round" />
	            </svg>
	        "#
	    }
	)
}

#[component]
pub fn HoverItem(hover_bg: String, radius: String, children: Element) -> Element {
	let mut is_hover = use_signal(|| false);

	let onmouseenter = move |_| is_hover.set(true);
	let onmouseleave = move |_| is_hover.set(false);

	let background = match is_hover() {
		true => hover_bg,
		false => "none".into(),
	};

	rsx!(
		rect {
	        onmouseenter,
	        onmouseleave,
	        background: background,
			corner_radius: radius,
            corner_smoothing: SMOOTHING.base,
			{children}
		}
	)
}
