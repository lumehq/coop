use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Error};
use common::profile::NostrProfile;
use global::{
    constants::{
        ALL_MESSAGES_SUB_ID, CLIENT_KEYRING, DEVICE_ANNOUNCEMENT_KIND, DEVICE_REQUEST_KIND,
        DEVICE_RESPONSE_KIND, DEVICE_SUB_ID, MASTER_KEYRING, NEW_MESSAGE_SUB_ID,
    },
    get_app_name, get_client, get_device_keys, get_device_name, set_device_keys,
};
use gpui::{
    div, px, relative, App, AppContext, Context, Entity, Global, ParentElement, Styled, Task,
    Window,
};
use nostr_sdk::prelude::*;
use smallvec::SmallVec;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    indicator::Indicator,
    notification::Notification,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Root, Sizable, StyledExt,
};

use crate::views::{app, onboarding, relays};

struct GlobalDevice(Entity<Device>);

impl Global for GlobalDevice {}

/// Current Device (Client)
///
/// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
#[derive(Debug)]
pub struct Device {
    /// Profile (Metadata) of current user
    profile: Option<NostrProfile>,
    /// Client Keys
    client_keys: Keys,
}

pub fn init(window: &mut Window, cx: &App) {
    // Initialize client keys
    let read_keys = cx.read_credentials(CLIENT_KEYRING);
    let window_handle = window.window_handle();

    cx.spawn(|cx| async move {
        let client_keys = if let Ok(Some((_, secret))) = read_keys.await {
            let secret_key = SecretKey::from_slice(&secret).unwrap();

            Keys::new(secret_key)
        } else {
            // Generate new keys and save them to keyring
            let keys = Keys::generate();

            if let Ok(write_keys) = cx.update(|cx| {
                cx.write_credentials(
                    CLIENT_KEYRING,
                    keys.public_key.to_hex().as_str(),
                    keys.secret_key().as_secret_bytes(),
                )
            }) {
                _ = write_keys.await;
            };

            keys
        };

        cx.update(|cx| {
            let entity = cx.new(|_| Device {
                profile: None,
                client_keys,
            });

            window_handle
                .update(cx, |_, window, cx| {
                    // Open the onboarding view
                    Root::update(window, cx, |this, window, cx| {
                        this.replace_view(onboarding::init(window, cx).into());
                        cx.notify();
                    });

                    window
                        .observe(&entity, cx, |this, window, cx| {
                            this.update(cx, |this, cx| {
                                this.on_device_change(window, cx);
                            });
                        })
                        .detach();
                })
                .ok();

            Device::set_global(entity, cx)
        })
        .ok();
    })
    .detach();
}

impl Device {
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalDevice>().map(|model| model.0.clone())
    }

    pub fn set_global(device: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalDevice(device));
    }

    pub fn profile(&self) -> Option<&NostrProfile> {
        self.profile.as_ref()
    }

    pub fn set_profile(&mut self, profile: NostrProfile, cx: &mut Context<Self>) {
        self.profile = Some(profile);
        cx.notify();
    }

    /// Login and set user signer
    pub fn login<T>(&self, signer: T, cx: &mut Context<Self>) -> Task<Result<(), Error>>
    where
        T: NostrSigner + 'static,
    {
        let client = get_client();

        // Set the user's signer as the main signer
        let login: Task<Result<NostrProfile, Error>> = cx.background_spawn(async move {
            // Use user's signer for main signer
            _ = client.set_signer(signer).await;

            // Verify nostr signer and get public key
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Fetch user's metadata
            let metadata = client
                .fetch_metadata(public_key, Duration::from_secs(2))
                .await
                .unwrap_or_default()
                .unwrap_or_default();

            // Get user's inbox relays
            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            let relays = if let Some(event) = client
                .fetch_events(filter, Duration::from_secs(2))
                .await?
                .first_owned()
            {
                let relays = event
                    .tags
                    .filter_standardized(TagKind::Relay)
                    .filter_map(|t| {
                        if let TagStandard::Relay(url) = t {
                            Some(url.to_owned())
                        } else {
                            None
                        }
                    })
                    .collect::<SmallVec<[RelayUrl; 3]>>();

                Some(relays)
            } else {
                None
            };

            let profile = NostrProfile::new(public_key, metadata).relays(relays);

            Ok(profile)
        });

        cx.spawn(|this, cx| async move {
            match login.await {
                Ok(user) => {
                    cx.update(|cx| {
                        this.update(cx, |this, cx| {
                            this.profile = Some(user);
                            cx.notify();
                        })
                        .ok();
                    })
                    .ok();

                    Ok(())
                }
                Err(e) => Err(e),
            }
        })
    }

    /// This function is called whenever the device is changed
    fn on_device_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.as_ref() else {
            // User not logged in, render the Onboarding View
            Root::update(window, cx, |this, window, cx| {
                this.replace_view(onboarding::init(window, cx).into());
                cx.notify();
            });

            return;
        };

        // Replace the Onboarding View with the Dock View
        Root::update(window, cx, |this, window, cx| {
            this.replace_view(app::init(window, cx).into());
            cx.notify();
        });

        // Get the user's messaging relays
        // If it is empty, user must setup relays
        let ready = profile.messaging_relays.is_some();

        cx.spawn_in(window, |this, mut cx| async move {
            cx.update(|window, cx| {
                if !ready {
                    this.update(cx, |this, cx| {
                        this.render_setup_relays(window, cx);
                    })
                    .ok();
                } else {
                    this.update(cx, |this, cx| {
                        this.start_subscription(cx);
                    })
                    .ok();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Initialize subscription for current user
    pub fn start_subscription(&self, cx: &Context<Self>) {
        let Some(profile) = self.profile() else {
            return;
        };

        let user = profile.public_key;
        let client = get_client();

        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let device_kind = Kind::Custom(DEVICE_ANNOUNCEMENT_KIND);

        // Create a device announcement filter
        let device = Filter::new().kind(device_kind).author(user).limit(1);

        // Create a contact list filter
        let contacts = Filter::new().kind(Kind::ContactList).author(user).limit(1);

        // Create a user's data filter
        let data = Filter::new()
            .author(user)
            .since(Timestamp::now())
            .kinds(vec![
                Kind::Metadata,
                Kind::InboxRelays,
                Kind::RelayList,
                device_kind,
            ]);

        // Create a filter for getting all gift wrapped events send to current user
        let msg = Filter::new().kind(Kind::GiftWrap).pubkey(user);

        // Create a filter to continuously receive new messages.
        let new_msg = Filter::new().kind(Kind::GiftWrap).pubkey(user).limit(0);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            // Only subscribe to the latest device announcement
            let sub_id = SubscriptionId::new(DEVICE_SUB_ID);
            client.subscribe_with_id(sub_id, device, Some(opts)).await?;

            // Only subscribe to the latest contact list
            client.subscribe(contacts, Some(opts)).await?;

            // Continuously receive new user's data since now
            client.subscribe(data, None).await?;

            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            client.subscribe_with_id(sub_id, msg, Some(opts)).await?;

            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
            client.subscribe_with_id(sub_id, new_msg, None).await?;

            Ok(())
        });

        cx.spawn(|_, _| async move {
            if let Err(e) = task.await {
                log::error!("Subscription error: {}", e);
            }
        })
        .detach();
    }

    /// Setup device
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn setup_device(&mut self, window: &mut Window, cx: &Context<Self>) {
        let Some(profile) = self.profile() else {
            return;
        };

        let client = get_client();
        let public_key = profile.public_key;
        let kind = Kind::Custom(DEVICE_ANNOUNCEMENT_KIND);
        let filter = Filter::new().kind(kind).author(public_key).limit(1);

        // Fetch device announcement events
        let fetch_announcement = cx.background_spawn(async move {
            if let Some(event) = client.database().query(filter).await?.first_owned() {
                Ok(event)
            } else {
                Err(anyhow!("Device Announcement not found."))
            }
        });

        cx.spawn_in(window, |this, mut cx| async move {
            if get_device_keys().await.is_some() {
                return;
            }

            if let Ok(event) = fetch_announcement.await {
                log::info!("Device Announcement: {:?}", event);
                if let Ok(task) = cx.update(|_, cx| cx.read_credentials(MASTER_KEYRING)) {
                    if let Ok(Some((pubkey, secret))) = task.await {
                        if let Some(n) = event
                            .tags
                            .find(TagKind::custom("n"))
                            .and_then(|t| t.content())
                            .map(|hex| hex.to_owned())
                        {
                            if n == pubkey {
                                cx.update(|window, cx| {
                                    this.update(cx, |this, cx| {
                                        this.reinit_master_keys(secret, window, cx);
                                    })
                                    .ok();
                                })
                                .ok();
                            } else {
                                cx.update(|window, cx| {
                                    this.update(cx, |this, cx| {
                                        this.request_keys(window, cx);
                                    })
                                    .ok();
                                })
                                .ok();
                            }

                            return;
                        }
                    }
                } else {
                    log::error!("Failed to read credentials");
                }

                log::info!("User cancelled keyring.")
            } else {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_master_keys(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Create a new master keys
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn set_master_keys(&self, window: &mut Window, cx: &Context<Self>) {
        log::info!("Device Announcement isn't found.");
        log::info!("Appoint this device as master");

        let client = get_client();
        let app_name = get_app_name();

        let task: Task<Result<Arc<Keys>, Error>> = cx.background_spawn(async move {
            let keys = Keys::generate();
            let kind = Kind::Custom(DEVICE_ANNOUNCEMENT_KIND);
            let client_tag = Tag::client(app_name);
            let pubkey_tag = Tag::custom(TagKind::custom("n"), vec![keys.public_key().to_hex()]);

            let event = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

            if let Err(e) = client.send_event_builder(event).await {
                log::error!("Failed to send Device Announcement: {}", e);
            } else {
                log::info!("Device Announcement has been sent");
            }

            Ok(Arc::new(keys))
        });

        cx.spawn_in(window, |_, mut cx| async move {
            if get_device_keys().await.is_some() {
                return;
            }

            if let Ok(keys) = task.await {
                // Update global state
                set_device_keys(keys.clone()).await;

                // Save keys
                if let Ok(task) = cx.update(|_, cx| {
                    cx.write_credentials(
                        MASTER_KEYRING,
                        keys.public_key().to_hex().as_str(),
                        keys.secret_key().as_secret_bytes(),
                    )
                }) {
                    if let Err(e) = task.await {
                        log::error!("Failed to write device keys to keyring: {}", e);
                    }
                };
            }
        })
        .detach();
    }

    /// Reinitialize master keys
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn reinit_master_keys(&self, secret: Vec<u8>, window: &mut Window, cx: &Context<Self>) {
        let Some(profile) = self.profile() else {
            return;
        };

        let client = get_client();
        let public_key = profile.public_key;

        let task: Task<Result<Arc<Keys>, Error>> = cx.background_spawn(async move {
            let secret_key = SecretKey::from_slice(&secret)?;
            let keys = Arc::new(Keys::new(secret_key));

            log::info!("Reappointing this device as master.");

            let filter = Filter::new()
                .kind(Kind::Custom(DEVICE_REQUEST_KIND))
                .author(public_key)
                .since(Timestamp::now());

            // Subscribe for new device requests
            _ = client.subscribe(filter, None).await;

            Ok(keys)
        });

        cx.spawn_in(window, |this, mut cx| async move {
            if let Ok(keys) = task.await {
                set_device_keys(keys).await;

                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.fetch_request(window, cx);
                    })
                    .ok();
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.request_keys(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Send a request to ask for device keys from the other Nostr client
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn request_keys(&self, window: &mut Window, cx: &Context<Self>) {
        let Some(profile) = self.profile() else {
            return;
        };

        let client = get_client();
        let app_name = get_app_name();

        let public_key = profile.public_key;
        let client_keys = self.client_keys.clone();

        let kind = Kind::Custom(DEVICE_REQUEST_KIND);
        let client_tag = Tag::client(app_name);
        let pubkey_tag = Tag::custom(
            TagKind::custom("pubkey"),
            vec![client_keys.public_key().to_hex()],
        );

        // Create a request event builder
        let builder = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            log::info!("Sent a request to ask for device keys from the other Nostr client");

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send device keys request: {}", e);
            } else {
                log::info!("Waiting for response...");
            }

            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

            let filter = Filter::new()
                .kind(Kind::Custom(DEVICE_RESPONSE_KIND))
                .author(public_key);

            // Getting all previous approvals
            client.subscribe(filter.clone(), Some(opts)).await?;

            // Continously receive the request approval
            client
                .subscribe(filter.since(Timestamp::now()), None)
                .await?;

            Ok(())
        });

        cx.spawn_in(window, |this, mut cx| async move {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.render_waiting_modal(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Fetch the latest request from the other Nostr client
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    fn fetch_request(&self, window: &mut Window, cx: &Context<Self>) {
        let Some(profile) = self.profile() else {
            return;
        };

        let client = get_client();
        let public_key = profile.public_key;

        let filter = Filter::new()
            .kind(Kind::Custom(DEVICE_REQUEST_KIND))
            .author(public_key)
            .limit(1);

        let task: Task<Result<Event, Error>> = cx.background_spawn(async move {
            let events = client.fetch_events(filter, Duration::from_secs(2)).await?;

            if let Some(event) = events.first_owned() {
                Ok(event)
            } else {
                Err(anyhow!("No request found"))
            }
        });

        cx.spawn_in(window, |this, mut cx| async move {
            if let Ok(event) = task.await {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.handle_request(event, window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Receive device keys approval from other Nostr client,
    /// then process and update device keys.
    pub fn handle_response(&self, event: Event, window: &mut Window, cx: &Context<Self>) {
        let local_signer = self.client_keys.clone().into_nostr_signer();

        let task = cx.background_spawn(async move {
            if let Some(public_key) = event.tags.public_keys().copied().last() {
                let secret = local_signer
                    .nip44_decrypt(&public_key, &event.content)
                    .await?;

                let keys = Arc::new(Keys::parse(&secret)?);

                // Update global state with new device keys
                set_device_keys(keys).await;

                log::info!("Received device keys from other client");

                Ok(())
            } else {
                Err(anyhow!("Failed to retrieve device key"))
            }
        });

        cx.spawn_in(window, |_, mut cx| async move {
            if let Err(e) = task.await {
                cx.update(|window, cx| {
                    window.push_notification(Notification::error(e.to_string()), cx);
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    window.push_notification(
                        Notification::success("Device Keys request has been approved"),
                        cx,
                    );
                })
                .ok();
            }
        })
        .detach();
    }

    /// Received device keys request from other Nostr client,
    /// then process the request and send approval response.
    pub fn handle_request(&self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let Some(public_key) = event
            .tags
            .find(TagKind::custom("pubkey"))
            .and_then(|tag| tag.content())
            .and_then(|content| PublicKey::parse(content).ok())
        else {
            return;
        };

        let client = get_client();
        let read_keys = cx.read_credentials(MASTER_KEYRING);
        let local_signer = self.client_keys.clone().into_nostr_signer();

        let device_name = event
            .tags
            .find(TagKind::Client)
            .and_then(|tag| tag.content())
            .unwrap_or("Other Device")
            .to_owned();

        let response = window.prompt(
            gpui::PromptLevel::Info,
            "Requesting Device Keys",
            Some(
                format!(
                    "{} is requesting shared device keys stored in this device",
                    device_name
                )
                .as_str(),
            ),
            &["Approve", "Deny"],
            cx,
        );

        cx.spawn_in(window, |_, cx| async move {
            match response.await {
                Ok(0) => {
                    if let Ok(Some((_, secret))) = read_keys.await {
                        let local_pubkey = local_signer.get_public_key().await?;

                        // Get device's secret key
                        let device_secret = SecretKey::from_slice(&secret)?;

                        // Encrypt device's secret key by using NIP-44
                        let content = local_signer
                            .nip44_encrypt(&public_key, &device_secret.to_secret_hex())
                            .await?;

                        // Create pubkey tag for other device (lowercase p)
                        let other_tag = Tag::public_key(public_key);

                        // Create pubkey tag for this device (uppercase P)
                        let local_tag = Tag::custom(
                            TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::P)),
                            vec![local_pubkey.to_hex()],
                        );

                        // Create event builder
                        let kind = Kind::Custom(DEVICE_RESPONSE_KIND);
                        let tags = vec![other_tag, local_tag];
                        let builder = EventBuilder::new(kind, content).tags(tags);

                        cx.background_spawn(async move {
                            if let Err(err) = client.send_event_builder(builder).await {
                                log::error!("Failed to send device keys to other client: {}", err);
                            } else {
                                log::info!("Sent device keys to other client");
                            }
                        })
                        .await;

                        Ok(())
                    } else {
                        Err(anyhow!("Device Keys not found"))
                    }
                }
                _ => Ok(()),
            }
        })
        .detach();
    }

    /// Show setup relays modal
    ///
    /// NIP-17: <https://github.com/nostr-protocol/nips/blob/master/17.md>
    pub fn render_setup_relays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = relays::init(window, cx);

        window.open_modal(cx, move |this, window, cx| {
            let is_loading = relays.read(cx).loading();

            this.keyboard(false)
                .closable(false)
                .width(px(420.))
                .title("Your Messaging Relays are not configured")
                .child(relays.clone())
                .footer(
                    div()
                        .p_2()
                        .border_t_1()
                        .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                        .child(
                            Button::new("update_inbox_relays_btn")
                                .label("Update")
                                .primary()
                                .bold()
                                .rounded(ButtonRounded::Large)
                                .w_full()
                                .loading(is_loading)
                                .on_click(window.listener_for(&relays, |this, _, window, cx| {
                                    this.update(window, cx);
                                })),
                        ),
                )
        });
    }

    /// Show waiting modal
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn render_waiting_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, _window, cx| {
            let msg = format!(
                "Please open {} and approve sharing device keys request.",
                get_device_name()
            );

            this.keyboard(false)
                .closable(false)
                .width(px(420.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .p_4()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .size_full()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .text_sm()
                                        .child(
                                            div()
                                                .font_semibold()
                                                .child("You're using a new device."),
                                        )
                                        .child(
                                            div()
                                                .text_color(
                                                    cx.theme()
                                                        .base
                                                        .step(cx, ColorScaleStep::ELEVEN),
                                                )
                                                .line_height(relative(1.3))
                                                .child(msg),
                                        ),
                                ),
                        ),
                )
                .footer(
                    div()
                        .p_4()
                        .border_t_1()
                        .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                        .w_full()
                        .flex()
                        .gap_2()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                        .child(Indicator::new().small())
                        .child("Waiting for approval ..."),
                )
        });
    }
}
