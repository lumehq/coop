use dioxus_radio::hooks::use_init_radio_station;
use freya::prelude::*;
use nostr_sdk::prelude::*;
use winit::platform::macos::WindowAttributesExtMacOS;

use crate::system::radio::{Data, DataChannel};
use crate::system::state::get_client;
use crate::theme::COLORS;
use crate::ui::UI;

mod system;
mod theme;
mod ui;
mod common;

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

fn app() -> Element {
	use_init_radio_station::<Data, DataChannel>(Data::default);

	use_future(move || async move {
		let client = get_client().await;
		let chat_id = SubscriptionId::new("chats");

		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Message { message, relay_url } = notification {
					if let RelayMessage::Event {
						subscription_id,
						event,
					} = message
					{
						if subscription_id == chat_id {
							println!("new chat: {}", event.id.to_hex())
						} else {
							println!("new event: {}", event.id.to_hex())
						}
					} else {
						println!("relay: {}", message.as_json())
					}
				}
				Ok(false)
			})
			.await
			.expect("TODO: panic message")
	});

	rsx!(
    rect {
      width: "100%",
      height: "100%",
      font_size: "14",
      UI {}
    }
  )
}
