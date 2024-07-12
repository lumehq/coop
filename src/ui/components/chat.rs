use dioxus_radio::hooks::use_radio;
use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::common::time_ago;
use crate::system::{get_chats, get_profile};
use crate::system::radio::{Data, DataChannel};
use crate::theme::{COLORS, SIZES, SMOOTHING};
use crate::ui::chats::Chats;

#[component]
pub fn ChannelList() -> Element {
	let mut radio = use_radio::<Data, DataChannel>(DataChannel::NewChat);

	use_future(move || async move {
		if let Ok(mut events) = get_chats().await {
			radio.write().chats.append(&mut events)
		}
	});

	rsx!(
    rect {
      width: "100%",
      height: "calc(100% - 45)",
      margin: "44 8 0 8",
      VirtualScrollView {
        length: radio.read().chats.len(),
        item_size: 56.0,
        direction: "vertical",
        builder: move |index, _: &Option<()>| {
          let event = &radio.read().chats[index];
          let pk = event.pubkey;
          let hex = event.pubkey.to_hex();

          rsx! {
            rect {
              key: "{hex}",
              height: "56",
              main_align: "center",
              cross_align: "center",
              onmouseenter: move |_| {
                spawn(async move {
                  // TODO: preload messages
                });
              },
              Link {
                to: Chats::Channel { id: hex.clone() },
                ActivableRoute {
                  route: Chats::Channel { id: hex },
                  exact: true,
                  Item { public_key: pk, created_at: event.created_at }
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
fn Item(public_key: PublicKey, created_at: Timestamp) -> Element {
	let is_active = use_activable_route();
	let metadata = use_resource(use_reactive!(|(public_key)| async move {
    get_profile(Some(&public_key)).await
  }));

	let time_ago = time_ago(created_at);

	let (background, color, label_color) = match is_active {
		true => (COLORS.neutral_200, COLORS.blue_500, COLORS.neutral_600),
		false => ("none", COLORS.black, COLORS.neutral_500),
	};

	match &*metadata.read_unchecked() {
		Some(Ok(profile)) => rsx!(
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
                background: COLORS.neutral_950
              }
            )
          }
        }
        rect {
          width: "fill",
          cross_align: "center",
          direction: "horizontal",
          padding: "0 0 0 8",
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
            padding: "2 0 0 0",
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
    ),
	}
}
