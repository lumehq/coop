use freya::prelude::*;
use winit::platform::macos::WindowAttributesExtMacOS;

use crate::system::state::get_client;
use crate::theme::COLORS;
use crate::ui::UI;

mod system;
mod theme;
mod ui;
mod common;

fn main() {
	let rt = tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
		.unwrap();

	let _guard = rt.enter();

	rt.spawn(async {
		let _ = get_client().await;
	});

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

fn app() -> Element {
	rsx!(
    rect {
      width: "100%",
      height: "100%",
      font_size: "14",
      UI {}
    }
  )
}
