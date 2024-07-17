use dioxus_router::prelude::{Outlet, Routable};
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::system::state::{CHATS, get_client, INBOXES, MESSAGES};
use crate::theme::COLORS;
use crate::ui::components::{Direction, Divider};
use crate::ui::components::chat::{ChannelForm, ChannelList, ChannelMembers, Messages};
use crate::ui::components::user::CurrentUser;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Chats {
	#[layout(Main)]
	#[route("/")]
	Welcome,
	#[route("/:hex")]
	Channel { hex: String },
	#[end_layout]
	#[route("/..route")]
	NotFound,
}

#[component]
fn Main() -> Element {
	let mut future = use_future(move || async move {
		let client = get_client().await;
		client
			.handle_notifications(|notification| async {
				if let RelayPoolNotification::Event { event, .. } = notification {
					if event.kind == Kind::GiftWrap {
						if let Ok(UnwrappedGift { rumor, sender }) =
							client.unwrap_gift_wrap(&event).await
						{
							let chats = CHATS.read().iter().map(|ev| ev.pubkey).collect::<Vec<_>>();

							if chats.iter().any(|pk| pk == &sender) {
								MESSAGES.write().push(rumor);
							} else {
								CHATS.write().push(rumor);
							}
						}
					}
				}
				Ok(false)
			})
			.await
			.expect("TODO: panic message");
	});

	use_drop(move || {
		future.cancel();
	});

	rsx!(
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
    )
}

#[component]
fn Welcome() -> Element {
	rsx!(
        rect {
            width: "100%",
            height: "100%",
            main_align: "center",
            cross_align :"center",
            label {
                "coop on nostr."
            }
        }
    )
}

#[component]
fn Channel(hex: ReadOnlySignal<String>) -> Element {
	let sender = PublicKey::from_hex(hex.read().to_string()).unwrap();

	let inbox = use_memo(use_reactive((&sender, &INBOXES()), |(sender, inboxes)| {
		inboxes.get(&sender).cloned()
	}));

	let scroll_controller = use_scroll_controller(|| ScrollConfig {
		default_vertical_position: ScrollPosition::End,
		..Default::default()
	});

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
                    ChannelMembers { sender }
                }
            },
            Divider { background: COLORS.neutral_200, direction: Direction::HORIZONTAL },
            rect {
                height: "calc(100% - 89)",
                ScrollView {
                    scroll_controller,
                    theme: theme_with!(ScrollViewTheme {
                        height: "100%".into(),
                    }),
                    show_scrollbar: false,
                    scroll_with_arrows: true,
                    reverse: true,
                    Messages { sender }
                },
            },
            match inbox() {
                Some(relays) => {
					match relays.is_empty() {
						false => rsx!( ChannelForm { sender, relays } ),
						true => rsx!(
		                    rect {
		                        width: "100%",
		                        height: "44",
		                        main_align: "center",
		                        cross_align: "center",
		                        rect {
		                            height: "28",
		                            background: COLORS.neutral_100,
		                            padding: "4 12",
		                            corner_radius: "28",
		                            main_align: "center",
		                            cross_align: "center",
		                            label {
		                                font_size: "11",
										"This user isn't have inbox relays, you cannot send message."
		                            }
		                        }
		                    }
		                )
					}
				},
                None => rsx!(
                    rect {
                        width: "100%",
                        height: "44",
                        main_align: "center",
                        cross_align: "center",
                        rect {
                            height: "28",
                            background: COLORS.neutral_100,
                            padding: "4 12",
                            corner_radius: "28",
                            main_align: "center",
                            cross_align: "center",
                            label {
                                font_size: "11",
								"Connecting to inbox relays..."
                            }
                        }
                    }
                ),
            }
        }
    )
}

#[component]
fn NotFound() -> Element {
	rsx!(rect {})
}
