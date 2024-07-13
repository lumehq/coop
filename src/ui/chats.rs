use dioxus_router::prelude::{
	Outlet,
	Routable,
};
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::system::get_chat_messages;
use crate::theme::COLORS;
use crate::ui::components::{Direction, Divider};
use crate::ui::components::chat::{ChannelList, ChannelMembers, Messages, NewMessages};
use crate::ui::components::user::CurrentUser;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Chats {
	// @formatter:off
	#[layout(Main)]
		#[route("/")]
		Welcome,
		#[route("/:id")]
		Channel { id: String },
	#[end_layout]
	#[route("/..route")]
	NotFound,
}

#[component]
fn Main() -> Element {
	rsx!(
		NativeRouter {
			rect {
				content: "fit",
	      height: "100%",
	      direction: "horizontal",
				rect {
	        width: "280",
	        height: "100%",
	        direction: "vertical",
					ChannelList {},
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
					Outlet::<Chats> {}
	      }
			}
		}
	)
}

#[component]
pub fn Welcome() -> Element {
	rsx!(
    rect {
			width: "100%",
      height: "100%",
      main_align: "center",
      cross_align: "center",
    }
	)
}

#[component]
pub fn Channel(id: String) -> Element {
	let sender = PublicKey::from_hex(id.clone()).unwrap();
	let messages = use_resource(use_reactive!(|(sender)| async move { get_chat_messages(sender).await }));

	let info_panel = use_signal(|| false);

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
					ChannelMembers { id }
				}
			},
			Divider { background: COLORS.neutral_200, direction: Direction::HORIZONTAL }
			rect {
				height: "calc(100% - 89)",
				ScrollView {
					theme: theme_with!(ScrollViewTheme {
		        height: "100%".into(),
		      }),
					show_scrollbar: false,
					scroll_with_arrows: true,
					match &*messages.read_unchecked() {
						Some(Ok(events)) => rsx!(
				      Messages { events: events.to_owned() }
							NewMessages { sender }
				    ),
						Some(Err(_)) => rsx!(
				      rect {
				        label {
				          "Error."
				        }
				      }
				    ),
						None => rsx!(
				      rect {
				        label {
				          "Loading..."
				        }
				      }
				    )
					}
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
			}
			rect {
	      width: "100%",
	      height: "44",
	      padding: "0 12 0 12",
	      main_align: "center",
	      cross_align: "center",
	      direction: "horizontal",
				// TODO: form
			}
    }
	)
}

#[component]
fn NotFound() -> Element {
	rsx!(
    rect {}
	)
}
