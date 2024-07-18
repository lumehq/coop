use freya::prelude::*;
use winit::platform::macos::WindowAttributesExtMacOS;

use coop::system::state::get_client;
use coop::theme::COLORS;
use coop::ui::App;

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
		App,
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
