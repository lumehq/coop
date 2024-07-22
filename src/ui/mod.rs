use dioxus_router::prelude::{Routable, Router};
use freya::prelude::*;

use crate::ui::{
	chats::Chats,
	connect_account::ConnectAccount,
	create_account::CreateAccount,
	import_account::ImportAccount,
	landing::Landing,
	new::New,
};

mod chats;
mod components;
mod connect_account;
mod create_account;
mod import_account;
mod landing;
mod new;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum AppRoute {
	#[route("/")]
	Landing,
	#[route("/new")]
	New,
	#[route("/create-account")]
	CreateAccount,
	#[route("/import-account")]
	ImportAccount,
	#[route("/connect-account")]
	ConnectAccount,
	#[route("/")]
	Chats,
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

#[component]
pub fn NotFound() -> Element {
	rsx!(rect {})
}
