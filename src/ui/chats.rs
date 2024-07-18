use dioxus_router::prelude::Outlet;
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::system::state::{CHATS, get_client, INBOXES, MESSAGES};
use crate::theme::{COLORS, GRID_ICON, PLUS_ICON};
use crate::ui::AppRoute;
use crate::ui::components::{Direction, Divider};
use crate::ui::components::chat::{ChannelForm, ChannelList, ChannelMembers, Messages, NewMessagePopup};
use crate::ui::components::user::CurrentUser;

#[component]
pub fn Main() -> Element {
	let grid_icon = static_bytes(GRID_ICON);
	let plus_icon = static_bytes(PLUS_ICON);

	let mut show_popup = use_signal(|| false);
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
		NewMessagePopup { show_popup }
        rect {
            content: "fit",
            height: "100%",
            direction: "horizontal",
            rect {
                width: "280",
                height: "100%",
                direction: "vertical",
				rect {
					height: "calc(100% - 45)",
					padding: "0 8",
					rect {
						width: "100%",
						height: "44",
						direction: "horizontal",
						main_align: "space-between",
						cross_align: "center",
						rect {}
						rect {
							direction: "horizontal",
							main_align: "center",
							cross_align: "center",
							Link {
								to: AppRoute::Chats,
								rect {
		                            width: "24",
		                            height: "24",
		                            main_align: "center",
		                            cross_align: "center",
									margin: "0 8 0 0",
		                            svg {
		                                width: "16",
		                                height: "16",
		                                svg_data: grid_icon,
		                            }
		                        }
							}
							Button {
				                onpress: move |_| show_popup.set(true),
				                theme: Some(ButtonThemeWith {
		                            background: Some(Cow::Borrowed(COLORS.neutral_200)),
		                            hover_background: Some(Cow::Borrowed(COLORS.neutral_400)),
		                            border_fill: Some(Cow::Borrowed(COLORS.neutral_200)),
		                            focus_border_fill: Some(Cow::Borrowed(COLORS.neutral_200)),
		                            corner_radius: Some(Cow::Borrowed("24 8 24 24")),
		                            font_theme: Some(FontThemeWith {
		                                color: Some(Cow::Borrowed(COLORS.black)),
		                            }),
		                            width: Some(Cow::Borrowed("44")),
		                            height: Some(Cow::Borrowed("24")),
		                            margin: Some(Cow::Borrowed("0")),
		                            padding: Some(Cow::Borrowed("0")),
		                            shadow: Some(Cow::Borrowed("none")),
		                        }),
		                        rect {
		                            width: "44",
		                            height: "24",
		                            corner_radius: "24 8 24 24",
		                            main_align: "center",
		                            cross_align: "center",
		                            svg {
		                                width: "16",
		                                height: "16",
		                                svg_data: plus_icon,
		                            }
		                        }
				            }
						}
					}
	                ChannelList {},
				}
                Divider { background: COLORS.neutral_200, direction: Direction::Horizontal },
                rect {
                    width: "100%",
                    height: "44",
                    CurrentUser {}
                }
            }
            Divider { background: COLORS.neutral_250, direction: Direction::Vertical },
            rect {
                width: "fill-min",
                height: "100%",
                background: COLORS.white,
                Outlet::<AppRoute> {}
            }
        }
    )
}

#[component]
pub fn Chats() -> Element {
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
pub fn Channel(hex: ReadOnlySignal<String>) -> Element {
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
            Divider { background: COLORS.neutral_200, direction: Direction::Horizontal },
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
										"This user doesn't have inbox relays. You cannot send messages to them."
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
pub fn NotFound() -> Element {
	rsx!(rect {})
}
