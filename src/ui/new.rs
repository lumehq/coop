use freya::prelude::*;

use crate::{
    theme::{COLORS, SIZES, SMOOTHING},
    ui::AppRoute,
};

#[component]
pub fn New() -> Element {
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
                        to: AppRoute::CreateAccount,
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
                        to: AppRoute::ConnectAccount,
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
                        to: AppRoute::ImportAccount,
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
