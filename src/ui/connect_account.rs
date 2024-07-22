use dioxus_router::prelude::navigator;
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::{
    system::connect_account,
    theme::{COLORS, SIZES, SMOOTHING},
    ui::{components::Spinner, AppRoute},
};

#[component]
pub fn ConnectAccount() -> Element {
    let mut uri = use_signal(String::new);
    let mut is_loading = use_signal(|| false);

    let client = consume_context::<&Client>();
    let nav = navigator();

    let onpointerup = move |_| {
        is_loading.set(true);

        spawn(async move {
            if connect_account(client, uri()).await.is_ok() {
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
                        "Nostr Connect"
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
                            "Connection String"
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
                            placeholder: "bunker://",
                            value: uri.read().clone(),
                            onchange: move |e| {
                                uri.set(e)
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
