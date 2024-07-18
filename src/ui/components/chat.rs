use std::cmp::Reverse;
use std::time::Duration;

use freya::prelude::*;
use itertools::Itertools;
use nostr_sdk::prelude::*;

use crate::common::{is_target, message_time, time_ago, use_debounce};
use crate::system::{get_chat_messages, get_chats, get_contact_list, get_inboxes, get_profile, preload, send_message};
use crate::system::state::{CHATS, CONTACT_LIST, CURRENT_USER, get_client, INBOXES, MESSAGES};
use crate::theme::{ARROW_UP_ICON, COLORS, SIZES, SMOOTHING};
use crate::ui::AppRoute;

#[component]
pub fn NewMessagePopup(show_popup: Signal<bool>) -> Element {
	use_future(|| async move {
		if let Ok(mut list) = get_contact_list().await {
			CONTACT_LIST.write().append(&mut list);
		};
	});

	rsx!(
        if *show_popup.read() {
            Popup {
                oncloserequest: move |_| {
                    show_popup.set(false)
                },
				theme: Some(PopupThemeWith {
					background: Some(Cow::Borrowed(COLORS.white)),
					color: None,
					cross_fill: None,
					width: Some(Cow::Borrowed("400")),
					height: Some(Cow::Borrowed("500"))
				}),
                rect {
					font_size: "16",
		            margin: "4 2 8 2",
		            font_weight: "600",
                    label {
                        "New message"
                    }
                }
                PopupContent {
                    VirtualScrollView {
                        length: CONTACT_LIST.read().len(),
                        item_size: 56.0,
                        direction: "vertical",
                        builder: move |index, _: &Option<()>| {
                            let contact = &CONTACT_LIST.read()[index];

                            rsx! {
                                ListItem { key: "{index}", public_key: contact.public_key, created_at: None }
                            }
                        }
                    }
                }
            }
        }
    )
}

#[component]
pub fn ChannelList() -> Element {
	let chats = use_memo(use_reactive((&CHATS(), ), |(events, )| {
		events
			.into_iter()
			.sorted_by_key(|ev| Reverse(ev.created_at))
			.collect::<Vec<_>>()
	}));

	use_future(move || async move {
		if let Ok(mut events) = get_chats().await {
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
                    let hex = event.pubkey.to_hex();

                    rsx! {
                        Link {
                            to: AppRoute::Channel { hex: hex.clone() },
                            ActivableRoute {
                                route: AppRoute::Channel { hex },
                                exact: true,
                                ListItem { public_key: pk, created_at: Some(event.created_at) }
                            }
                        }
                    }
                }
            }
        }
    )
}

#[component]
fn ListItem(public_key: PublicKey, created_at: Option<Timestamp>) -> Element {
	let is_active = use_activable_route();
	let metadata = use_resource(use_reactive!(|(public_key)| async move {
        get_profile(Some(&public_key)).await
    }));

	let mut debounce = use_debounce(Duration::from_millis(500), move |pk| {
		spawn(async move {
			if let Ok(relays) = get_inboxes(pk).await {
				INBOXES.write().insert(pk, relays);
				let _ = preload(public_key).await;
			};
		});
	});

	let (background, color, label_color) = match is_active {
		true => (COLORS.neutral_200, COLORS.blue_500, COLORS.neutral_600),
		false => ("none", COLORS.black, COLORS.neutral_500),
	};

	let time_ago = created_at.map(time_ago);

	match &*metadata.read_unchecked() {
		Some(Ok(profile)) => rsx!(
            rect {
                onmouseenter: move |_| {
                    debounce.action(public_key)
                },
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
                    match &profile.picture {
                        Some(picture) => rsx!(
                            NetworkImage {
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
                    match time_ago {
                        Some(t) => rsx!(
                            rect {
                                padding: "1 0 0 0",
                                label {
                                    color: label_color,
                                    font_size: "12",
                                    text_align: "right",
                                    "{t}"
                                }
                            }
                        ),
                        None => rsx!( rect {} )
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
					width: "32",
					height: "32",
					corner_radius: "32",
                    background: COLORS.neutral_200,
				}
				rect {
					margin: "0 0 0 8",
					label {
	                    "Loading.."
	                }
				}
            }
        ),
	}
}

#[component]
pub fn ChannelMembers(sender: PublicKey) -> Element {
	let metadata = use_resource(use_reactive!(|(sender)| async move {
        get_profile(Some(&sender)).await
    }));

	let mut is_hover = use_signal(|| false);

	let onmouseenter = move |_| is_hover.set(true);

	let onmouseleave = move |_| is_hover.set(false);

	let background = match is_hover() {
		true => COLORS.neutral_100,
		false => "none",
	};

	match &*metadata.read_unchecked() {
		Some(Ok(profile)) => rsx!(
            rect {
                onmouseenter,
                onmouseleave,
                background: background,
                corner_radius: SIZES.sm,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.xs,
                direction: "horizontal",
                cross_align: "center",
                rect {
                    width: "24",
                    height: "24",
                    margin: "0 4 0 0",
                    match &profile.picture {
                        Some(picture) => rsx!(
                            NetworkImage {
                                theme: Some(NetworkImageThemeWith { width: Some(Cow::from("24")), height: Some(Cow::from("24")) }),
                                url: format!("https://wsrv.nl/?url={}&w=100&h=100&fit=cover&mask=circle&output=png", picture).parse::<Url>().unwrap(),
                            }
                        ),
                        None => rsx!(
                            rect {
                                width: "24",
                                height: "24",
                                corner_radius: "24",
                                background: COLORS.neutral_950
                            }
                        )
                    }
                }
                rect {
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
                corner_radius: SIZES.sm,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.base,
                width: "100%",
                direction: "horizontal",
                cross_align: "center",
                label {
                    "Cannot load profile: {err}"
                }
            }
        ),
		None => rsx!(
            rect {
                corner_radius: SIZES.sm,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.base,
                width: "100%",
                content: "fit",
                direction: "horizontal",
                cross_align: "center",
                rect {
                    margin: "0 4 0 0",
                    width: "24",
                    height: "24",
                    corner_radius: "28",
                    background: COLORS.neutral_200
                }
                rect {
                    width: "60",
                    height: "10",
                    corner_radius: "2",
                    background: COLORS.neutral_200,
                }
            }
        ),
	}
}

#[component]
pub fn ChannelForm(sender: PublicKey, relays: ReadOnlySignal<Vec<String>>) -> Element {
	let arrow_up_icon = static_bytes(ARROW_UP_ICON);

	let mut value = use_signal(String::new);

	let onpress = move |_| {
		spawn(async move {
			let message = value.read().to_string();

			if message.is_empty() {
				return;
			};

			if let Ok(event) = send_message(sender, message, relays()).await {
				MESSAGES.write().push(event);
				value.set(String::new())
			};
		});
	};

	rsx!(
        rect {
            width: "100%",
            height: "44",
            padding: "0 12 0 12",
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

#[component]
pub fn Messages(sender: PublicKey) -> Element {
	let messages = use_resource(use_reactive!(|(sender)| async move {
        get_chat_messages(sender).await
    }));

	let new_messages = use_memo(use_reactive((&sender, ), |(sender, )| {
		let receiver = PublicKey::from_hex(CURRENT_USER.read().as_str()).unwrap();
		MESSAGES
			.read()
			.clone()
			.into_iter()
			.filter_map(|ev| {
				if is_target(&sender, &ev.tags) || is_target(&receiver, &ev.tags) {
					Some(ev)
				} else {
					None
				}
			})
			.collect::<Vec<_>>()
	}));

	use_future(move || async move {
		let client = get_client().await;
		let subscription_id = SubscriptionId::new(format!("channel_{}", sender.to_hex()));

		let messages = Filter::new().kind(Kind::GiftWrap).pubkey(sender).limit(0);

		client
			.subscribe_with_id(subscription_id, vec![messages], None)
			.await
			.expect("TODO: panic message");
	});

	use_drop(move || {
		spawn(async move {
			let client = get_client().await;
			let subscription_id = SubscriptionId::new(format!("channel_{}", sender.to_hex()));

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
