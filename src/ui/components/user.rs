use std::str::FromStr;

use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::system::{get_profile, login};
use crate::system::state::CURRENT_USER;
use crate::theme::{COLORS, SIZES, SMOOTHING};

#[component]
pub fn LoginUser(id: String) -> Element {
	let public_key = PublicKey::from_str(&id).unwrap();
	let metadata = use_resource(use_reactive!(|(public_key)| async move {
    get_profile(Some(&public_key)).await
  }));

	let mut is_hover = use_signal(|| false);
	let mut is_loading = use_signal(|| false);

	let onpointerup = move |_| {
		is_loading.set(true);

		spawn(async move {
			if let Ok(user) = login(public_key).await {
				*CURRENT_USER.write() = user
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
        corner_radius: SIZES.sm,
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
				rect {
					width: "32",
					match is_loading() {
						true => rsx!(rect {
							label {
			          "..."
			        }
						}),
						false => rsx!()
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
				content: "fit",
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

#[component]
pub fn CurrentUser() -> Element {
	let metadata = use_resource(|| async move { get_profile(None).await });

	rsx!(
    rect {
      match &*metadata.read_unchecked() {
        Some(Ok(profile)) => rsx!(
          rect {
						width: "100%",
						height: "44",
            padding: "0 12",
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
