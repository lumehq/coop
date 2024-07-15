use dioxus_router::prelude::Routable;
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::system::state::{CHATS, get_client, MESSAGES};
use crate::theme::COLORS;
use crate::ui::components::{Direction, Divider};
use crate::ui::components::chat::{ChannelForm, ChannelList, ChannelMembers, Messages};
use crate::ui::components::user::CurrentUser;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Chats {
	#[route("/")]
	Main,
	#[route("/..route")]
	NotFound,
}

#[component]
fn Main() -> Element {
	let current_channel = use_signal(String::new);

	let mut future = use_future(move || async move {
		let client = get_client().await;

		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Event { event, .. } = notification {
					if event.kind == Kind::GiftWrap {
						if let Ok(UnwrappedGift { rumor, sender }) = client.unwrap_gift_wrap(&event).await {
							let chats = CHATS.read().iter().map(|ev| ev.pubkey).collect::<Vec<_>>();

							if chats.iter().any(|pk| pk == &sender) {
								MESSAGES.write().push(rumor);
							} else {
								CHATS.write().push(rumor);
							}
						}
					}
				}
				Ok(false)
			})
			.await
			.expect("TODO: panic message");
	});

	use_drop(move || {
		future.cancel();
	});

	rsx!(
		rect {
			content: "fit",
			height: "100%",
			direction: "horizontal",
			rect {
		        width: "280",
		        height: "100%",
		        direction: "vertical",
				ChannelList { current_channel },
				Divider { background: COLORS.neutral_200, direction: Direction::HORIZONTAL },
	            rect {
					width: "100%",
					height: "44",
					CurrentUser {}
	            }
			}
			Divider { background: COLORS.neutral_250, direction: Direction::VERTICAL },
			rect {
		        width: "fill-min",
		        height: "100%",
		        background: COLORS.white,
				match current_channel.read().is_empty() {
					false => rsx!( Channel { current_channel } ),
					true => rsx!(
				        rect {
							width: "100%",
							height: "100%",
							main_align: "center",
							cross_align: "center",
						}
					),
				}
            }
		}
	)
}

#[component]
pub fn Channel(current_channel: Signal<String>) -> Element {
	let sender = PublicKey::from_hex(current_channel.read().to_string()).unwrap();
	let info_panel = use_signal(|| false);

	let scroll_controller = use_scroll_controller(|| ScrollConfig {
		default_vertical_position: ScrollPosition::End,
		..Default::default()
	});

	rsx!(
	    rect {
			width: "100%",
	        height: "100%",
			rect {
				width: "100%",
				height: "44",
				padding: "0 4",
				main_align: "space-between",
				rect {
					height: "44",
					main_align: "center",
					ChannelMembers { sender }
				}
			},
			Divider { background: COLORS.neutral_200, direction: Direction::HORIZONTAL },
			rect {
				height: "calc(100% - 89)",
				ScrollView {
					scroll_controller,
					theme: theme_with!(ScrollViewTheme {
			            height: "100%".into(),
			        }),
					show_scrollbar: false,
					scroll_with_arrows: true,
					Messages { sender }
				},
				match info_panel() {
					true => rsx!(
						Divider { background: COLORS.neutral_200, direction: Direction::VERTICAL },
						rect {
							width: "250",
							height: "100%",
							label {
								"Panel"
							}
						}
					),
					false => rsx!( rect {})
				}
			},
			ChannelForm { sender }
        }
	)
}

#[component]
fn NotFound() -> Element {
	rsx!(
        rect {}
	)
}
