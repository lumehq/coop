use chrono::Local;
use dioxus_router::prelude::Routable;
use freya::prelude::*;

use crate::common::get_accounts;
use crate::theme::{COLORS, PLUS_ICON, SIZES, SMOOTHING};
use crate::ui::components::HoverItem;
use crate::ui::components::user::LoginUser;

#[derive(Routable, Clone, PartialEq)]
pub enum Authenticate {
	#[route("/")]
	Landing,
	#[route("/new")]
	NewAccount,
	#[route("/create")]
	Create,
	#[route("/import")]
	Import,
	#[route("/connect")]
	Connect,
}

#[component]
pub fn Landing() -> Element {
	let plus_icon = static_bytes(PLUS_ICON);
	let accounts = get_accounts();
	let current_date: String = Local::now().format("%A, %B %d").to_string();

	match accounts.is_empty() {
		true => rsx!(
			NewAccount {}
		),
		false => {
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
                            for npub in accounts {
                                LoginUser { id: npub }
                            }
							Link {
		                        to: Authenticate::NewAccount,
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
	}
}

#[component]
fn NewAccount() -> Element {
	rsx!(
	    rect {
	        width: "100%",
	        height: "100%",
			main_align: "center",
			cross_align: "center",
	        rect {
	            width: "320",
	            background: COLORS.white,
				shadow: "0 10 15 -3 rgb(0, 0, 0, 10), 0 4 6 -4 rgb(0, 0, 0, 10)",
                corner_radius: SIZES.lg,
                corner_smoothing: SMOOTHING.base,
                padding: SIZES.sm,
	            Link {
					to: Authenticate::Create,
					ActivableRoute {
						route: Authenticate::Create,
						SidebarItem {
							label {
								"Create a new identity"
							}
						},
					}
				},
	            Link {
					to: Authenticate::Connect,
					ActivableRoute {
						route: Authenticate::Connect,
						SidebarItem {
							label {
								"Continue with Nostr Connect"
							}
						},
					}
				},
	            Link {
					to: Authenticate::Import,
					ActivableRoute {
						route: Authenticate::Import,
						SidebarItem {
							label {
								"Continue with Private Key"
							}
						},
					}
				},
			}
		}
	)
}

#[component]
pub fn Create() -> Element {
	rsx!(
		rect {
	        width: "100%",
	        height: "100%",
	        main_align: "center",
	        cross_align: "center",
	    }
	)
}

#[component]
pub fn Import() -> Element {
	rsx!(
		rect {
	        width: "100%",
	        height: "100%",
	        main_align: "center",
	        cross_align: "center",
	    }
	)
}

#[component]
pub fn Connect() -> Element {
	rsx!(
	    rect {
	        width: "100%",
	        height: "100%",
	        main_align: "center",
	        cross_align: "center",
	    }
    )
}
