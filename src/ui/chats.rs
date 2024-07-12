use dioxus_router::prelude::{
	Outlet,
	Routable,
};
use freya::prelude::*;

use crate::theme::COLORS;
use crate::ui::components::{Direction, Divider};
use crate::ui::components::chat::ChannelList;
use crate::ui::components::user::CurrentUser;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Chats {
	// @formatter:off
	#[layout(AppSidebar)]
		#[route("/")]
		Welcome,
		#[route("/:id")]
		Channel { id: String },
	#[end_layout]
	#[route("/..route")]
	NotFound,
}

#[component]
fn AppSidebar() -> Element {
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
fn NotFound() -> Element {
	rsx!(
    rect {}
	)
}
