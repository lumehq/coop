use dioxus_router::prelude::navigator;
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::{
    system::{create_account, update_profile},
    theme::{COLORS, SIZES, SMOOTHING},
    ui::{components::Spinner, AppRoute},
};

#[component]
pub fn CreateAccount() -> Element {
    let mut avatar = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut is_loading = use_signal(|| false);

    let client = consume_context::<&Client>();
    let nav = navigator();

    let onpointerup = move |_| {
        is_loading.set(true);

        spawn(async move {
            if create_account(client).await.is_ok() {
                let picture = Url::parse(&avatar.read().to_string());
                let metadata = match picture {
                    Ok(url) => Metadata::new()
                        .display_name(display_name.read().to_string())
                        .picture(url),
                    Err(_) => Metadata::new().display_name(display_name.read().to_string()),
                };

                let _ = tokio::spawn(async move {
                    let _ = update_profile(client, metadata).await;
                })
                .await;

                nav.replace(AppRoute::Landing);
            }
        });
    };

    rsx!(
        rect {
            width: "100%",
            height: "100%",
            main_align: "center",
            cross_align: "center",
            rect {
                width: "320",
                rect {
                    width: "fill-min",
                    cross_align: "center",
                    text_align: "center",
                    direction: "vertical",
                    label {
                        font_size: "16",
                        font_weight: "600",
                        width: "100%",
                        "New Identity"
                    },
                },
                rect {
                    width: "100%",
                    background: COLORS.white,
                    shadow: "0 10 15 -3 rgb(0, 0, 0, 10), 0 4 6 -4 rgb(0, 0, 0, 10)",
                    corner_radius: SIZES.lg,
                    corner_smoothing: SMOOTHING.base,
                    padding: SIZES.lg,
                    margin: "20 0 0 0",
                    rect {
                        width: "100%",
                        label {
                            color: COLORS.neutral_700,
                            margin: "0 0 4",
                            font_size: "12",
                            font_weight: "500",
                            "Avatar"
                        },
                        Input {
                            theme: Some(InputThemeWith {
                                border_fill: Some(Cow::Borrowed(COLORS.neutral_100)),
                                background: Some(Cow::Borrowed(COLORS.white)),
                                hover_background: Some(Cow::Borrowed(COLORS.white)),
                                corner_radius: Some(Cow::Borrowed("8")),
                                font_theme: Some(FontThemeWith {
                                    color: Some(Cow::Borrowed(COLORS.black)),
                                }),
                                placeholder_font_theme: Some(FontThemeWith {
                                    color: Some(Cow::Borrowed(COLORS.neutral_500)),
                                }),
                                margin: Some(Cow::Borrowed("0")),
                                shadow: Some(Cow::Borrowed("none")),
                                width: Some(Cow::Borrowed("100%")),
                            }),
                            placeholder: "https://",
                            value: avatar.read().clone(),
                            onchange: move |e| {
                                avatar.set(e)
                            }
                        }
                    }
                    rect {
                        width: "100%",
                        margin: "12 0 0 0",
                        label {
                            color: COLORS.neutral_700,
                            margin: "0 0 4",
                            font_size: "12",
                            font_weight: "500",
                            "Name"
                        },
                        Input {
                            theme: Some(InputThemeWith {
                                border_fill: Some(Cow::Borrowed(COLORS.neutral_100)),
                                background: Some(Cow::Borrowed(COLORS.white)),
                                hover_background: Some(Cow::Borrowed(COLORS.white)),
                                corner_radius: Some(Cow::Borrowed("8")),
                                font_theme: Some(FontThemeWith {
                                    color: Some(Cow::Borrowed(COLORS.black)),
                                }),
                                placeholder_font_theme: Some(FontThemeWith {
                                    color: Some(Cow::Borrowed(COLORS.neutral_500)),
                                }),
                                margin: Some(Cow::Borrowed("0")),
                                shadow: Some(Cow::Borrowed("none")),
                                width: Some(Cow::Borrowed("100%")),
                            }),
                            placeholder: "Alice",
                            value: display_name.read().clone(),
                            onchange: move |e| {
                                display_name.set(e)
                            }
                        }
                    }
                    rect {
                        onpointerup,
                        margin: "12 0 0 0",
                        background: COLORS.blue_500,
                        color: COLORS.white,
                        corner_radius: SIZES.base,
                        corner_smoothing: SMOOTHING.base,
                        padding: SIZES.base,
                        width: "100%",
                        height: "32",
                        cross_align: "center",
                        main_align: "center",
                        match *is_loading.read() {
                            true => rsx! (
                                Spinner {}
                            ),
                            false => rsx! (
                                label {
                                    "Continue"
                                }
                            )
                        }
                    }
                }
            }
        }
    )
}
