use dioxus_radio::hooks::use_radio;
use dioxus_router::prelude::Router;
use freya::prelude::*;

use crate::system::radio::{Data, DataChannel};
use crate::ui::authenticate::Authenticate;
use crate::ui::chats::Chats;

mod authenticate;
mod chats;
mod components;

#[component]
pub fn UI() -> Element {
	let radio = use_radio::<Data, DataChannel>(DataChannel::SetCurrentUser);

	rsx!(
    rect {
      width: "100%",
      height: "100%",
      match radio.read().current_user.is_empty() {
        false => rsx!(Router::<Chats> {}),
        true => rsx!(Router::<Authenticate> {}),
      }
    }
  )
}
