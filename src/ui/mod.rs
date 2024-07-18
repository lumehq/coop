use dioxus_router::prelude::{Routable, Router};
use freya::prelude::*;

use crate::system::state::CURRENT_USER;
use crate::ui::authenticate::*;
use crate::ui::chats::*;

mod authenticate;
mod chats;
mod components;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum AppRoute {
	#[route("/")]
	Landing,
	#[route("/new")]
	NewAccount,
	#[route("/create")]
	Create,
	#[route("/import")]
	Import,
	#[route("/connect")]
	Connect,
	// @formatter:off
	#[layout(Main)]
		#[route("/chats")]
		Chats,
		#[route("/chats/:hex")]
		Channel { hex: String },
		#[end_layout]
	#[route("/..route")]
	NotFound,
}

#[component]
pub fn App() -> Element {
	rsx!(
        rect {
            width: "100%",
            height: "100%",
            font_size: "14",
            Router::<AppRoute> {}
        }
    )
}
