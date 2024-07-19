use chrono::Local;
use dioxus_router::prelude::navigator;
use freya::prelude::*;
use nostr_sdk::{Metadata, Url};

use crate::common::get_accounts;
use crate::system::{connect_account, create_account, import_key, login, update_profile};
use crate::theme::{COLORS, PLUS_ICON, SIZES, SMOOTHING};
use crate::ui::components::user::LoginUser;
use crate::ui::components::{HoverItem, Spinner};
use crate::ui::AppRoute;

#[component]
pub fn Landing() -> Element {
    let accounts = use_signal(get_accounts);
    let plus_icon = static_bytes(PLUS_ICON);
    let current_date = Local::now().format("%A, %B %d").to_string();
    let nav = navigator();

    use_effect(move || {
        if accounts.read().is_empty() {
            nav.replace(AppRoute::NewAccount);
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
                        LoginUser { id: npub }
                    }
                    Link {
                        to: AppRoute::NewAccount,
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
pub fn NewAccount() -> Element {
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
                        "Direct Message for Nostr."
                    },
                },
                rect {
                    width: "100%",
                    margin: "20 0 0 0",
                    Link {
                        to: AppRoute::Create,
                        rect {
                            background: COLORS.blue_500,
                            color: COLORS.white,
                            corner_radius: SIZES.base,
                            corner_smoothing: SMOOTHING.base,
                            shadow: "0 10 15 -3 rgb(0, 0, 0, 10), 0 4 6 -4 rgb(0, 0, 0, 10)",
                            padding: SIZES.base,
                            width: "100%",
                            height: "32",
                            cross_align: "center",
                            label {
                                "Create a new identity"
                            }
                        }
                    },
                    Link {
                        to: AppRoute::Connect,
                        rect {
                            margin: "12 0 0 0",
                            background: COLORS.white,
                            corner_radius: SIZES.base,
                            corner_smoothing: SMOOTHING.base,
                            shadow: "0 10 15 -3 rgb(0, 0, 0, 10), 0 4 6 -4 rgb(0, 0, 0, 10)",
                            padding: SIZES.base,
                            width: "100%",
                            height: "32",
                            cross_align: "center",
                            label {
                                "Login with Nostr Connect"
                            }
                        }
                    },
                    Link {
                        to: AppRoute::Import,
                        rect {
                            margin: "12 0 0 0",
                            label {
                                color: COLORS.neutral_600,
                                text_align: "center",
                                font_size: "12",
                                "Login with Private Key (not recommended)"
                            }
                        }
                    },
                }
            }
        }
    )
}

#[component]
pub fn Create() -> Element {
    let mut avatar = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut is_loading = use_signal(|| false);

    let nav = navigator();

    let onpointerup = move |_| {
        is_loading.set(true);

        spawn(async move {
            if create_account().await.is_ok() {
                let picture = Url::parse(&avatar.read().to_string());
                let metadata = match picture {
                    Ok(url) => Metadata::new()
                        .display_name(display_name.read().to_string())
                        .picture(url),
                    Err(_) => Metadata::new().display_name(display_name.read().to_string()),
                };

                let _ = tokio::spawn(async move {
                    let _ = update_profile(metadata).await;
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

#[component]
pub fn Connect() -> Element {
    let mut uri = use_signal(String::new);
    let mut is_loading = use_signal(|| false);

    let nav = navigator();

    let onpointerup = move |_| {
        is_loading.set(true);

        spawn(async move {
            if connect_account(uri()).await.is_ok() {
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

#[component]
pub fn Import() -> Element {
    let mut nsec = use_signal(String::new);
    let mut is_loading = use_signal(|| false);

    let nav = navigator();

    let onpointerup = move |_| {
        is_loading.set(true);

        spawn(async move {
            if import_key(nsec()).await.is_ok() {
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
                        "Import Key"
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
                            "Private Key"
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
                            placeholder: "nsec1...",
                            value: nsec.read().clone(),
                            onchange: move |e| {
                                nsec.set(e)
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
