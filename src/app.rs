use crate::system::radio::{Data, DataChannel};
use crate::system::state::{get_client, ClientAction};
use crate::ui::authenticate::Authenticate;
use dioxus_radio::hooks::{use_init_radio_station, use_radio};
use dioxus_router::prelude::Router;
use freya::prelude::*;
use nostr_sdk::prelude::*;
use crate::ui::chats::Chats;

pub fn app() -> Element {
	use_init_radio_station::<Data, DataChannel>(Data::default);

	use_coroutine(|_rx: UnboundedReceiver<ClientAction>| async move {
		let client = get_client().await;
		let chat_id = SubscriptionId::new("chats");

		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Message { message, relay_url } = notification {
					if let RelayMessage::Event {
						subscription_id,
						event,
					} = message
					{
						if subscription_id == chat_id {
							println!("new chat: {}", event.id.to_hex())
						} else {
							println!("new event: {}", event.id.to_hex())
						}
					} else if let RelayMessage::Auth { challenge } = message {
						if client.auth(challenge, relay_url.clone()).await.is_ok() {
							println!("Authenticated to '{relay_url}' relay.")
						}
					} else {
						println!("relay: {}", message.as_json())
					}
				}
				Ok(false)
			})
			.await
			.expect("TODO: panic message")
	});

	let radio = use_radio::<Data, DataChannel>(DataChannel::SetCurrentUser);

	rsx!(
    rect {
      width: "100%",
      height: "100%",
      font_size: "13",
      match radio.read().current_user.is_empty() {
        false => rsx!(Router::<Chats> {}),
        true => rsx!(Router::<Authenticate> {}),
      }
    }
  )
}
