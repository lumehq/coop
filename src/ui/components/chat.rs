use freya::prelude::*;
use nostr_sdk::prelude::*;

use crate::common::{is_target, message_time, time_ago};
use crate::system::{get_chats, get_profile, preload};
use crate::system::state::{CHATS, CURRENT_USER, get_client};
use crate::theme::{COLORS, SIZES, SMOOTHING};
use crate::ui::chats::Chats;

#[component]
pub fn ChannelList() -> Element {
	use_future(move || async move {
		if let Ok(mut events) = get_chats().await {
			CHATS.write().append(&mut events)
		}
	});

	rsx!(
    rect {
      width: "100%",
      height: "calc(100% - 45)",
      margin: "44 8 0 8",
      VirtualScrollView {
        length: CHATS.read().len(),
        item_size: 56.0,
        direction: "vertical",
        builder: move |index, _: &Option<()>| {
          let event = &CHATS.read()[index];
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
pub fn ChannelMembers(id: String) -> Element {
	let public_key = PublicKey::from_hex(id.clone()).unwrap();
	let metadata = use_resource(use_reactive!(|(public_key)| async move {
    get_profile(Some(&public_key)).await
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
          width: "28",
          height: "28",
          margin: "0 4 0 0",
          match &profile.picture {
            Some(picture) => rsx!(
              NetworkImage {
                theme: Some(NetworkImageThemeWith { width: Some(Cow::from("28")), height: Some(Cow::from("28")) }),
                url: format!("https://wsrv.nl/?url={}&w=100&h=100&fit=cover&mask=circle&output=png", picture).parse::<Url>().unwrap(),
              }
            ),
            None => rsx!(
              rect {
                width: "28",
                height: "28",
                corner_radius: "28",
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
          width: "28",
          height: "28",
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
fn Item(public_key: PublicKey, created_at: Timestamp) -> Element {
	let time_ago = time_ago(created_at);
	let is_active = use_activable_route();

	let metadata = use_resource(use_reactive!(|(public_key)| async move {
    get_profile(Some(&public_key)).await
  }));

	let (background, color, label_color) = match is_active {
		true => (COLORS.neutral_200, COLORS.blue_500, COLORS.neutral_600),
		false => ("none", COLORS.black, COLORS.neutral_500),
	};

	let onmouseenter = move |_| {
		tokio::spawn(async move {
			let _ = preload(public_key).await;
		});
	};

	match &*metadata.read_unchecked() {
		Some(Ok(profile)) => rsx!(
      rect {
        onmouseenter,
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

#[component]
pub fn Messages(events: Vec<UnsignedEvent>) -> Element {
	rsx!(
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
  )
}

#[component]
pub fn NewMessages(sender: PublicKey) -> Element {
	let new_messages = use_signal::<Vec<UnsignedEvent>>(Vec::new);

	use_future(move || async move {
		let client = get_client().await;
		let signer = client.signer().await.unwrap();
		let receiver = signer.public_key().await.unwrap();
		let id = SubscriptionId::new(format!("{}_{}", sender.to_hex(), receiver.to_hex()));

		let messages = Filter::new()
			.kind(Kind::GiftWrap)
			.pubkeys(vec![sender, receiver])
			.limit(0);

		if client.subscribe_with_id(id, vec![messages], None).await.is_ok() {
			client
				.handle_notifications(|notification| async {
					if let RelayPoolNotification::Event { event, .. } = notification {
						if event.kind == Kind::GiftWrap {
							if let Ok(UnwrappedGift { rumor, .. }) = client.unwrap_gift_wrap(&event).await {
								if rumor.kind == Kind::PrivateDirectMessage && is_target(&receiver, &rumor.tags) {
									new_messages.write_unchecked().push(rumor)
								}
							}
						}
					}
					Ok(false)
				})
				.await
				.expect("TODO: panic message");
		}
	});

	rsx!(
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
		false => ("start", "24 24 8 24", COLORS.neutral_100, COLORS.black)
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
