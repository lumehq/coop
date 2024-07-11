use dioxus_router::prelude::Routable;
use freya::prelude::*;

#[derive(Routable, Clone, PartialEq)]
pub enum Authenticate {
	#[route("/")]
	Landing,
	#[route("/create")]
	Create,
	#[route("/import")]
	Import,
	#[route("/connect")]
	Connect,
}

#[component]
pub fn Landing() -> Element {
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
pub fn Create() -> Element {
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
pub fn Import() -> Element {
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
pub fn Connect() -> Element {
	rsx!(
    rect {
			width: "100%",
      height: "100%",
      main_align: "center",
      cross_align: "center",
    }
	)
}
