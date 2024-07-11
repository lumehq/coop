use freya::prelude::*;
use winit::platform::macos::WindowAttributesExtMacOS;
use crate::app::app;
use crate::theme::COLORS;

mod system;
mod theme;
mod app;
mod ui;

fn main() {
	launch_cfg(
		app,
		LaunchConfig::<()>::new()
			.with_size(860.0, 650.0)
			.with_background(COLORS.neutral_100)
			.with_window_attributes(|window| {
				window
					.with_titlebar_transparent(true)
					.with_fullsize_content_view(true)
					.with_title_hidden(true)
					.with_content_protected(false) // TODO: set to true
			}),
	);
}