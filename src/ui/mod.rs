use dioxus_router::prelude::Router;
use freya::prelude::*;

use crate::system::state::CURRENT_USER;
use crate::ui::authenticate::Authenticate;
use crate::ui::chats::Chats;

mod authenticate;
mod chats;
mod components;

#[component]
pub fn UI() -> Element {
	rsx!(
    rect {
      width: "100%",
      height: "100%",
      match CURRENT_USER.read().is_empty() {
        false => rsx!(Router::<Chats> {}),
        true => rsx!(Router::<Authenticate> {}),
      }
    }
  )
}
