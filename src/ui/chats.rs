use dioxus_router::prelude::{
	Outlet,
	Routable,
};
use freya::prelude::*;

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
				Outlet::<Chats> {}
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
