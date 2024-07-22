use std::str::FromStr;

use chrono::Local;
use dioxus_router::prelude::navigator;
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::{
    common::get_accounts,
    system::{get_profile, login, state::CURRENT_USER},
    theme::{COLORS, PLUS_ICON, SIZES, SMOOTHING},
    ui::{
        AppRoute,
        components::{HoverItem, Spinner},
    },
};

#[component]
pub fn Landing() -> Element {
	let plus_icon = static_bytes(PLUS_ICON);
	let current_date = Local::now().format("%A, %B %d").to_string();
	let nav = navigator();

	let mut accounts = use_signal(Vec::new);

	use_effect(move || {
		let local_accounts = get_accounts();

		if local_accounts.is_empty() {
			nav.replace(AppRoute::Landing);
		} else {
			*accounts.write() = local_accounts;
		}
	});

	rsx!(
        rect {
            width: "100%",
            height: "100%",
            main_align: "center",
            cross_align: "center",
            rect {
                width: "320",
                content: "fit",
                rect {
                    width: "fill-min",
                    cross_align: "center",
                    text_align: "center",
                    direction: "vertical",
                    label {
                        color: COLORS.neutral_700,
                        font_size: "16",
                        width: "100%",
                        margin: "0 0 4 0",
                        {current_date}
                    },
                    label {
                        font_size: "16",
                        font_weight: "600",
                        width: "100%",
                        "Welcome Back"
                    },
                }
                rect {
                    width: "100%",
                    background: COLORS.white,
                    shadow: "0 10 15 -3 rgb(0, 0, 0, 10), 0 4 6 -4 rgb(0, 0, 0, 10)",
                    corner_radius: SIZES.lg,
                    corner_smoothing: SMOOTHING.base,
                    padding: SIZES.sm,
                    margin: "20 0 0 0",
                    for (_, npub) in accounts.read().iter().enumerate() {
                        User { id: npub }
                    }
                    Link {
                        to: AppRoute::New,
                        HoverItem {
                            hover_bg: COLORS.neutral_100,
                            radius: SIZES.base,
                            rect {
                                padding: SIZES.base,
                                width: "100%",
                                content: "fit",
                                direction: "horizontal",
                                cross_align: "center",
                                rect {
                                    width: "36",
                                    height: "36",
                                    corner_radius: "36",
                                    margin: "0 6 0 0",
                                    background: COLORS.neutral_200,
                                    main_align: "center",
                                    cross_align: "center",
                                    svg {
                                        width: "14",
                                        height: "14",
                                        svg_data: plus_icon
                                    }
                                },
                                label {
                                    font_size: "13",
                                    "Add an account"
                                }
                            }
                        }
                    }
                }
            }
        }
    )
}

#[component]
fn User(id: String) -> Element {
	let client = consume_context::<&Client>();
	let public_key = PublicKey::from_str(&id).unwrap();
	let nav = navigator();

	let metadata = use_resource(use_reactive!(|(public_key)| async move {
        get_profile(client, Some(&public_key)).await
    }));

	let mut is_hover = use_signal(|| false);
	let mut is_loading = use_signal(|| false);

	let onpointerup = move |_| {
		is_loading.set(true);

		spawn(async move {
			if let Ok(user) = login(client, public_key).await {
				*CURRENT_USER.write() = user.to_hex();
				nav.replace(AppRoute::Chats);
			}
		});
	};

	let onmouseenter = move |_| is_hover.set(true);

	let onmouseleave = move |_| is_hover.set(false);

	let background = match is_hover() {
		true => COLORS.neutral_100,
		false => "none",
	};

	match &*metadata.read_unchecked() {
		Some(Ok(profile)) => rsx!(
            rect {
                onpointerup,
                onmouseenter,
                onmouseleave,
                background: background,
                corner_radius: SIZES.base,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.base,
                width: "100%",
                content: "fit",
                direction: "horizontal",
                cross_align: "center",
                main_align: "space-between",
                rect {
                    width: "fill",
                    direction: "horizontal",
                    cross_align: "center",
                    rect {
                        width: "36",
                        height: "36",
                        margin: "0 6 0 0",
                        match &profile.picture {
                            Some(picture) => rsx!(
                                NetworkImage {
                                    loading: Some(rsx!( Spinner {} )),
                                    theme: Some(NetworkImageThemeWith { width: Some(Cow::from("36")), height: Some(Cow::from("36")) }),
                                    url: format!("https://wsrv.nl/?url={}&w=100&h=100&fit=cover&mask=circle&output=png", picture).parse::<Url>().unwrap(),
                                }
                            ),
                            None => rsx!(
                                rect {
                                    width: "36",
                                    height: "36",
                                    corner_radius: "36",
                                    background: COLORS.neutral_950
                                }
                            )
                        }
                    }
                    rect {
                        width: "fill",
                        rect {
                            margin: "0 0 2 0",
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
                        label {
                            color: COLORS.neutral_600,
                            font_size: "12",
                            "npub1..."
                        }
                    }
                },
                match is_loading() {
                    true => rsx!(
                        Spinner {}
                    ),
                    false => rsx!( rect {} )
                }
            }
        ),
		Some(Err(err)) => rsx!(
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
                    rect {
                        width: "36",
                        height: "36",
                        corner_radius: "36",
                        background: COLORS.neutral_200
                    }
                }
                rect {
                    label {
                        "Cannot load profile: {err}"
                    }
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
                    rect {
                        width: "36",
                        height: "36",
                        corner_radius: "36",
                        background: COLORS.neutral_200
                    }
                }
                rect {
                    rect {
                        width: "80",
                        height: "10",
                        corner_radius: "2",
                        background: COLORS.neutral_200,
                        margin: "0 0 4 0",
                    }
                    rect {
                        width: "40",
                        height: "10",
                        corner_radius: "2",
                        background: COLORS.neutral_200
                    }
                }
            }
        ),
	}
}
