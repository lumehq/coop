use dioxus_router::prelude::Routable;
use freya::prelude::*;

use crate::common::get_accounts;
use crate::theme::{COLORS, SIZES, SMOOTHING};
use crate::ui::components::user::LoginUser;

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
	let accounts = get_accounts();

	match accounts.is_empty() {
		true => {
			rsx!(
		    rect {
					width: "100%",
		      height: "100%",
		      main_align: "center",
		      cross_align: "center",
					rect {
						width: "320",
						background: COLORS.white,
						corner_radius: SIZES.lg,
						corner_smoothing: SMOOTHING.base,
						padding: SIZES.sm,
						Link {
		          to: Authenticate::Create,
		          ActivableRoute {
		            route: Authenticate::Create,
		            SidebarItem {
		              label {
		                "Create a new identity"
		              }
		            },
		          }
		        },
						Link {
		          to: Authenticate::Connect,
		          ActivableRoute {
		            route: Authenticate::Connect,
		            SidebarItem {
		              label {
		                "Continue with Nostr Connect"
		              }
		            },
		          }
		        },
						Link {
		          to: Authenticate::Import,
		          ActivableRoute {
		            route: Authenticate::Import,
		            SidebarItem {
		              label {
		                "Continue with Private Key"
		              }
		            },
		          }
		        },
					}
		    }
			)
		}
		false => {
			rsx!(
		    rect {
					width: "100%",
		      height: "100%",
		      main_align: "center",
		      cross_align: "center",
					rect {
						width: "320",
						main_align: "center",
						label {
							text_align: "center",
							font_size: "16",
							font_weight: "600",
							"Welcome Back"
						},
						rect {
							width: "100%",
							background: COLORS.white,
							corner_radius: SIZES.lg,
							corner_smoothing: SMOOTHING.base,
							padding: SIZES.sm,
							margin: "16 0 0 0",
							for npub in accounts {
								LoginUser { id: npub }
							}
						}
					}
		    }
			)
		}
	}
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
