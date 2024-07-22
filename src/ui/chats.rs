use std::cmp::Reverse;

use freya::prelude::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::{
	common::{is_target, message_time, time_ago},
	system::{
		get_chat_messages, get_chats,
		get_inboxes, get_profile,
		preload, send_message,
		state::{CHATS, CURRENT_USER, INBOXES, MESSAGES},
	},
	theme::{ARROW_UP_ICON, COLORS, GRID_ICON, PLUS_ICON, SIZES, SMOOTHING},
	ui::{
		AppRoute,
		components::{Direction, Divider, Spinner},
	},
};

#[derive(Clone, Copy)]
struct ChatState {
	id: Option<PublicKey>,
	loading: bool,
}

#[component]
pub fn Chats() -> Element {
	use_context_provider(|| Signal::new(ChatState { id: None, loading: false }));

	let client = consume_context::<&Client>();

	let grid_icon = static_bytes(GRID_ICON);
	let plus_icon = static_bytes(PLUS_ICON);

	let mut show_popup = use_signal(|| false);
	let mut future = use_future(move || async move {
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
        // NewMessagePopup { show_popup }
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
                        main_align: "end",
                        cross_align: "center",
                        rect {
                            direction: "horizontal",
                            main_align: "center",
                            cross_align: "center",
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
                    ChatList {},
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
                Inner {}
            }
        }
    )
}

#[component]
fn Inner() -> Element {
	let state = use_context::<Signal<ChatState>>();
	let open = state.read().id.is_none();

	match open {
		true => rsx!( rect { label { "coop on nostr." } } ),
		false => rsx!( Chat {} )
	}
}

#[component]
fn ChatList() -> Element {
	let client = consume_context::<&Client>();

	let chats = use_memo(use_reactive(&CHATS(), |events| {
		events
			.into_iter()
			.sorted_by_key(|ev| Reverse(ev.created_at))
			.collect::<Vec<_>>()
	}));

	use_future(move || async move {
		if let Ok(mut events) = get_chats(client).await {
			CHATS.write().append(&mut events)
		}
	});

	rsx!(
        NativeRouter {
            VirtualScrollView {
                length: chats.len(),
                item_size: 56.0,
                direction: "vertical",
                builder: move |index, _: &Option<()>| {
                    let event = &chats.get(index).unwrap();
                    let pk = event.pubkey;

                    rsx! {
                        ChatListItem { public_key: pk, created_at: event.created_at }
                    }
                }
            }
        }
    )
}

#[component]
fn ChatListItem(public_key: PublicKey, created_at: Timestamp) -> Element {
	let client = consume_context::<&Client>();
	let mut state = use_context::<Signal<ChatState>>();
	let mut is_loading = use_signal(|| false);

	let metadata = use_resource(use_reactive!(|(public_key)| async move {
        get_profile(client, Some(&public_key)).await
    }));

	let onpointerup = move |_| {
		is_loading.set(true);

		spawn(async move {
			if let Ok(relays) = get_inboxes(client, public_key).await {
				INBOXES.write().insert(public_key, relays);
			};

			tokio::spawn(async move {
				preload(client, public_key).await.expect("TODO: panic message");
			});

			state.write().id = Some(public_key);
			*is_loading.write() = false;
		});
	};

	let is_active = if let Some(id) = state.read().id {
		id == public_key
	} else {
		false
	};

	let (background, color, label_color) = match is_active {
		true => (COLORS.neutral_200, COLORS.blue_500, COLORS.neutral_600),
		false => ("none", COLORS.black, COLORS.neutral_500),
	};

	let time_ago = time_ago(created_at);

	match &*metadata.read_unchecked() {
		Some(Ok(profile)) => rsx!(
            rect {
				onpointerup,
                background: background,
                height: "56",
                content: "fit",
                corner_radius: SIZES.base,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.base,
                direction: "horizontal",
                cross_align: "center",
                rect {
                    width: "32",
                    height: "32",
					main_align: "center",
					cross_align: "center",
                    match &*is_loading.read() {
						true => rsx!( Spinner {} ),
						false => match &profile.picture {
	                        Some(picture) => rsx!(
	                            NetworkImage {
									loading: Some(rsx!( Spinner {} )),
	                                theme: Some(NetworkImageThemeWith { width: Some(Cow::from("32")), height: Some(Cow::from("32")) }),
	                                url: format!("https://wsrv.nl/?url={}&w=100&h=100&fit=cover&mask=circle&output=png", picture).parse::<Url>().unwrap(),
	                            }
	                        ),
	                        None => rsx!(
	                            rect {
	                                width: "32",
	                                height: "32",
	                                corner_radius: "32",
	                                background: "linear-gradient(90deg, #9FCCFA 0%, #0974F1 100%)",
	                            }
	                        )
	                    }
					}
                }
                rect {
                    width: "fill",
                    cross_align: "center",
                    direction: "horizontal",
                    margin: "0 0 0 8",
                    rect {
                        color: color,
                        font_weight: "500",
                        match &profile.display_name {
                            Some(display_name) => rsx!(
	                            label {
	                                max_lines: "1",
	                                text_overflow: "ellipsis",
	                                "{display_name}"
	                            }
	                        ),
	                        None => rsx!(
	                            rect {
	                                match &profile.name {
	                                    Some(name) => rsx!(
	                                        label {
	                                            max_lines: "1",
	                                            text_overflow: "ellipsis",
	                                            "{name}"
	                                        }
	                                    ),
	                                    None => rsx!(
                                            label {
                                                "Anon"
                                            }
										)
									}
								}
							)
						}
                    },
                    rect {
                        padding: "1 0 0 0",
						label {
                            color: label_color,
                            font_size: "12",
							text_align: "right",
                            "{time_ago}"
                        }
                    }
                }
            }
        ),
		Some(Err(_)) => rsx!(
            rect {
                background: background,
                height: "56",
                content: "fit",
                corner_radius: SIZES.base,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.base,
                direction: "horizontal",
                cross_align: "center",
                rect {
                    width: "32",
                    height: "32",
                    corner_radius: "32",
                    background: COLORS.neutral_200,
                }
                rect {
                    margin: "0 0 0 8",
                    label {
                        "Error."
                    }
                }
            }
        ),
		None => rsx!(
            rect {
                background: background,
                height: "56",
                content: "fit",
                corner_radius: SIZES.base,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.base,
                direction: "horizontal",
                cross_align: "center",
                rect {
                    margin: "0 4 0 0",
                    rect {
                        width: "32",
                        height: "32",
                        corner_radius: "32",
                        background: COLORS.neutral_200
                    }
                }
                rect {
                    width: "80",
                    height: "10",
                    corner_radius: "2",
                    background: COLORS.neutral_200,
                    margin: "0 0 4 0",
                }
            }
        ),
	}
}

#[component]
fn CurrentUser() -> Element {
	let client = consume_context::<&Client>();
	let metadata = use_resource(move || async move { get_profile(client, None).await });

	rsx!(
        rect {
            width: "100%",
            height: "44",
            padding: "0 12",
            match &*metadata.read_unchecked() {
                Some(Ok(profile)) => rsx!(
                    rect {
                        height: "44",
                        direction: "horizontal",
                        cross_align: "center",
                        NetworkImage {
                            theme: Some(NetworkImageThemeWith { width: Some(Cow::from("32")), height: Some(Cow::from("32")) }),
                            url: format!("https://wsrv.nl/?url={}&w=200&h=200&fit=cover&mask=circle&output=png", profile.picture.clone().unwrap()).parse::<Url>().unwrap(),
                        },
                        rect {
                            margin: "0 0 0 8",
                            font_weight: "500",
                            direction: "horizontal",
                            cross_align: "center",
                            match &profile.display_name {
                                Some(display_name) => rsx!(
                                    label {
                                        "{display_name}"
                                    }
                                ),
                                None => rsx!(
                                    rect {
                                        match &profile.name {
                                            Some(name) => rsx!(
                                                label {
                                                    "{name}"
                                                }
                                            ),
                                            None => rsx!(
                                                label {
                                                    "Anon"
                                                }
                                            )
                                        }
                                    }
                                )
                            }
                        }
                    }
                ),
                Some(Err(err)) => rsx!(
                    rect {
                        label {
                            "{err}"
                        }
                    }
                ),
                None => rsx!(
                    rect {
                        label {
                            "Loading..."
                        }
                    }
                )
            }
        }
    )
}

#[component]
fn Chat() -> Element {
	let state = use_context::<Signal<ChatState>>();
	let id = state.read().id.unwrap();

	rsx!(
		rect {
			width: "100%",
            height: "100%",
			rect {
                height: "calc(100% - 44)",
                ScrollView {
                    theme: theme_with!(ScrollViewTheme {
                        height: "100%".into(),
                    }),
                    show_scrollbar: false,
                    reverse: true,
                    Messages { id }
                },
            },
			match INBOXES.read().get(&id) {
				Some(relays) => {
					match relays.is_empty() {
						false => rsx!( Form { sender: id, relays: relays.to_owned() } ),
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
fn Messages(id: PublicKey) -> Element {
	let client = consume_context::<&Client>();

	let messages = use_resource(use_reactive!(|(id)| async move {
        get_chat_messages(client, id).await
    }));

	let new_messages = use_memo(use_reactive((&id, &MESSAGES()), |(id, messages)| {
		let receiver = PublicKey::from_hex(CURRENT_USER.read().as_str()).unwrap();
		messages
			.into_iter()
			.filter_map(|ev| {
				if is_target(&id, &ev.tags) || is_target(&receiver, &ev.tags) {
					Some(ev)
				} else {
					None
				}
			})
			.collect::<Vec<_>>()
	}));

	use_future(move || async move {
		let subscription_id = SubscriptionId::new(format!("channel_{}", id.to_hex()));
		let messages = Filter::new().kind(Kind::GiftWrap).pubkey(id).limit(0);

		client
			.subscribe_with_id(subscription_id, vec![messages], None)
			.await
			.expect("TODO: panic message");
	});

	use_drop(move || {
		spawn(async move {
			let subscription_id = SubscriptionId::new(format!("channel_{}", id.to_hex()));
			client.unsubscribe(subscription_id).await;
		});
	});

	rsx!(
        match &*messages.read_unchecked() {
            Some(Ok(events)) => rsx!(
                for (index, event) in events.iter().enumerate() {
                    rect {
                        key: "{index}",
                        width: "100%",
                        margin: "8 0 8 0",
                        rect {
                            width: "100%",
                            padding: "0 8 0 8",
                            direction: "horizontal",
                            cross_align: "center",
                            MessageContent { public_key: event.pubkey.to_hex(), content: event.content.clone() }
                            MessageTime { created_at: event.created_at }
                        }
                    }
                }
            ),
            Some(Err(_)) => rsx!(
                rect {
                    label {
                        "Error."
                    }
                }
            ),
            None => rsx!(
                rect {
                    label {
                        "Loading..."
                    }
                }
            )
        }
        for (index, event) in new_messages.read().iter().enumerate() {
            rect {
                key: "{index}",
                width: "100%",
                margin: "8 0 8 0",
                rect {
                    width: "100%",
                    padding: "0 8 0 8",
                    direction: "horizontal",
                    cross_align: "center",
                    MessageContent { public_key: event.pubkey.to_hex(), content: event.content.clone() }
                    MessageTime { created_at: event.created_at }
                }
            }
        }
    )
}

#[component]
fn MessageContent(public_key: String, content: String) -> Element {
	let is_self = public_key == *CURRENT_USER.read();

	let (align, radius, background, color) = match is_self {
		true => ("end", "24 8 24 24", COLORS.blue_500, COLORS.white),
		false => ("start", "24 24 8 24", COLORS.neutral_100, COLORS.black),
	};

	rsx!(
        rect {
            width: "calc(100% - 64)",
            cross_align: align,
            rect {
                corner_radius: radius,
                corner_smoothing: SMOOTHING.base,
                background: background,
                padding: "10 12 10 12",
                label {
                    color: color,
                    line_height: "1.5",
                    "{content}"
                }
            }
        }
    )
}

#[component]
fn MessageTime(created_at: Timestamp) -> Element {
	let message_time = message_time(created_at);

	rsx!(
        rect {
            width: "64",
            label {
                color: COLORS.neutral_600,
                font_size: "11",
                text_align: "right",
                "{message_time}"
            }
        }
    )
}

#[component]
fn Form(sender: PublicKey, relays: Vec<String>) -> Element {
	let client = consume_context::<&Client>();
	let arrow_up_icon = static_bytes(ARROW_UP_ICON);

	let mut value = use_signal(String::new);

	let onpress = move |_| {
		let relays = relays.to_owned();

		spawn(async move {
			if value.read().is_empty() {
				return;
			};

			if send_message(client, sender, value(), relays).await.is_ok() {
				value.set(String::new())
			};
		});
	};

	rsx!(
        rect {
            width: "100%",
            height: "44",
            padding: "0 12",
            main_align: "center",
            cross_align: "center",
            direction: "horizontal",
            rect {
                width: "100%",
                direction: "horizontal",
                main_align: "center",
                cross_align: "center",
                Input {
                    theme: Some(InputThemeWith {
                        border_fill: Some(Cow::Borrowed(COLORS.neutral_200)),
                        background: Some(Cow::Borrowed(COLORS.white)),
                        hover_background: Some(Cow::Borrowed(COLORS.white)),
                        corner_radius: Some(Cow::Borrowed("44")),
                        font_theme: Some(FontThemeWith {
                            color: Some(Cow::Borrowed(COLORS.black)),
                        }),
                        placeholder_font_theme: Some(FontThemeWith {
                            color: Some(Cow::Borrowed(COLORS.neutral_500)),
                        }),
                        margin: Some(Cow::Borrowed("0")),
                        shadow: Some(Cow::Borrowed("none")),
                        width: Some(Cow::Borrowed("calc(100% - 56)")),
                    }),
                    placeholder: "Message...",
                    value: value.read().clone(),
                    onchange: move |e| {
                        value.set(e)
                    }
                }
                rect {
                    width: "56",
                    height: "32",
                    main_align: "center",
                    cross_align: "end",
                    Button {
                        onpress,
                        theme: Some(ButtonThemeWith {
                            background: Some(Cow::Borrowed(COLORS.neutral_200)),
                            hover_background: Some(Cow::Borrowed(COLORS.neutral_400)),
                            border_fill: Some(Cow::Borrowed(COLORS.neutral_200)),
                            focus_border_fill: Some(Cow::Borrowed(COLORS.neutral_200)),
                            corner_radius: Some(Cow::Borrowed("32")),
                            font_theme: Some(FontThemeWith {
                                color: Some(Cow::Borrowed(COLORS.black)),
                            }),
                            width: Some(Cow::Borrowed("44")),
                            height: Some(Cow::Borrowed("32")),
                            margin: Some(Cow::Borrowed("0")),
                            padding: Some(Cow::Borrowed("0")),
                            shadow: Some(Cow::Borrowed("none")),
                        }),
                        rect {
                            width: "44",
                            height: "32",
                            corner_radius: "32",
                            main_align: "center",
                            cross_align: "center",
                            svg {
                                width: "16",
                                height: "16",
                                svg_data: arrow_up_icon,
                            }
                        }
                    }
                }
            }
        }
    )
}
