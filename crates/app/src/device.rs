use std::time::Duration;

use anyhow::{anyhow, Context as AnyContext, Error};
use common::profile::NostrProfile;
use global::{
    constants::{
        ALL_MESSAGES_SUB_ID, CLIENT_KEYRING, DATA_SUB_ID, DEVICE_ANNOUNCEMENT_KIND,
        DEVICE_REQUEST_KIND, DEVICE_RESPONSE_KIND, MASTER_KEYRING, NEW_MESSAGE_SUB_ID,
    },
    get_app_name, get_client, set_device_keys,
};
use gpui::{
    div, px, relative, App, AppContext, AsyncApp, Context, Entity, Global, ParentElement, Styled,
    Task, Window,
};
use nostr_sdk::prelude::*;
use smallvec::SmallVec;
use smol::future::FutureExt;
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

                    // Observe login behavior
                    window
                        .observe(&entity, cx, |this, window, cx| {
                            this.update(cx, |this, cx| {
                                this.on_login(window, cx);
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

    fn on_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

        let pubkey = profile.public_key;
        let client_keys = self.client_keys.clone();
        let read_task = cx.read_credentials(MASTER_KEYRING).boxed();
        let window_handle = window.window_handle();
        let entity = cx.entity();

        // User's messaging relays not found
        if profile.messaging_relays.is_none() {
            window_handle
                .update(cx, |_, window, cx| {
                    entity.update(cx, |this, cx| {
                        this.render_setup_relays(window, cx);
                    });
                })
                .ok();

            return;
        };

        cx.spawn(|this, cx| async move {
            // Initialize subscription for current user
            _ = Device::subscribe(pubkey, &cx).await;

            // Initialize master keys for current user
            if let Ok(Some(keys)) = Device::master_keys(read_task, pubkey, &cx).await {
                set_device_keys(keys).await;
            } else {
                // Send request for master keys
                if Device::request_keys(pubkey, client_keys, &cx).await.is_ok() {
                    cx.update(|cx| {
                        window_handle
                            .update(cx, |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.render_waiting_modal(window, cx);
                                })
                                .ok();
                            })
                            .ok()
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Receive device keys approval from other Nostr client,
    /// then process and update device keys.
    pub fn handle_response(&self, event: Event, window: &mut Window, cx: &Context<Self>) {
        let handle = window.window_handle();
        let local_signer = self.client_keys.clone().into_nostr_signer();

        let task = cx.background_spawn(async move {
            if let Some(public_key) = event.tags.public_keys().copied().last() {
                let secret = local_signer
                    .nip44_decrypt(&public_key, &event.content)
                    .await?;

                let keys = Keys::parse(&secret)?;

                // Update global state with new device keys
                set_device_keys(keys).await;

                log::info!("Received device keys from other client");

                Ok(())
            } else {
                Err(anyhow!("Failed to retrieve device key"))
            }
        });

        cx.spawn(|_, cx| async move {
            if let Err(e) = task.await {
                _ = cx.update(|cx| {
                    _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    });
                });
            } else {
                _ = cx.update(|cx| {
                    _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(
                            Notification::success("Device Keys request has been approved"),
                            cx,
                        );
                    });
                });
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

        let name = event
            .tags
            .find(TagKind::Client)
            .and_then(|tag| tag.content())
            .map(|content| content.to_string());

        let message = if let Some(client_name) = name {
            format!("Device Keys shared with {}", client_name)
        } else {
            "Device Keys shared with other client".to_owned()
        };

        let client = get_client();
        let handle = window.window_handle();
        let read_keys = cx.read_credentials(MASTER_KEYRING);

        let approve_task = cx.background_spawn(async move {
            if let Ok(Some((_, secret))) = read_keys.await {
                let local_keys = Keys::generate();
                let local_pubkey = local_keys.public_key();
                let local_signer = local_keys.into_nostr_signer();

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
                let tags = vec![other_tag, local_tag];
                let builder =
                    EventBuilder::new(Kind::Custom(DEVICE_RESPONSE_KIND), content).tags(tags);

                // Send event
                client.send_event_builder(builder).await?;
                log::info!("Sent device keys to other client");

                Ok(())
            } else {
                Err(anyhow!("Device Keys not found"))
            }
        });

        cx.spawn(|_, cx| async move {
            if approve_task.await.is_ok() {
                _ = cx.update(|cx| {
                    _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(Notification::info(message), cx);
                    });
                });
            }
        })
        .detach();
    }

    pub fn render_setup_relays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, window, cx| {
            let relays = relays::init(window, cx);
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

    pub fn render_waiting_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, _window, cx| {
            let msg = format!(
                "Please open {} and approve sharing device keys request.",
                get_app_name()
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

    fn request_keys(
        current_user: PublicKey,
        client_keys: Keys,
        cx: &AsyncApp,
    ) -> Task<Result<(), Error>> {
        cx.background_spawn(async move {
            log::info!("Sent a request to ask for device keys from the other Nostr client");

            let client = get_client();
            let app_name = get_app_name();

            let kind = Kind::Custom(DEVICE_REQUEST_KIND);

            let client_tag = Tag::client(app_name);
            let pubkey_tag = Tag::custom(
                TagKind::custom("pubkey"),
                vec![client_keys.public_key().to_hex()],
            );

            // Create a request event builder
            let builder = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send device keys request: {}", e);
            }

            log::info!("Waiting for response...");

            let resp = Filter::new()
                .kind(Kind::Custom(DEVICE_RESPONSE_KIND))
                .author(current_user)
                .since(Timestamp::now());

            // Continously receive the request approval
            client.subscribe(resp, None).await?;

            Ok(())
        })
    }

    /// Initialize master keys (encryption keys) for current user
    ///
    /// NIP-4E: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    #[allow(clippy::type_complexity)]
    fn master_keys(
        task: BoxedFuture<'static, Result<Option<(String, Vec<u8>)>, Error>>,
        current_user: PublicKey,
        cx: &AsyncApp,
    ) -> Task<Result<Option<Keys>, Error>> {
        cx.background_spawn(async move {
            let client = get_client();
            let app_name = get_app_name();

            let kind = Kind::Custom(DEVICE_ANNOUNCEMENT_KIND);
            let filter = Filter::new().kind(kind).author(current_user).limit(1);
            let client_tag = Tag::client(app_name);

            // Fetch device announcement events
            let events = client.database().query(filter).await?;

            // Found device announcement event,
            // that means user is already used another Nostr client or re-install this client
            if let Some(event) = events.first_owned() {
                // Check if master keys are found in keyring
                if let Ok(Some((_, secret))) = task.await {
                    let secret_key = SecretKey::from_slice(&secret)?;
                    let keys = Keys::new(secret_key);
                    let device_pubkey = keys.public_key();

                    log::info!("Device's Public Key: {:?}", device_pubkey);

                    let n_tag = event.tags.find(TagKind::custom("n")).context("Not found")?;
                    let content = n_tag.content().context("Not found")?;
                    let target_pubkey = PublicKey::parse(content)?;

                    // If device public key matches announcement public key, re-appoint as master
                    if device_pubkey == target_pubkey {
                        log::info!("Re-appointing this device as master");
                        return Ok(Some(keys));
                    }
                    // Otherwise fall through to request device keys
                }

                Ok(None)
            } else {
                log::info!("Device announcement is not found, appoint this device as master");

                let keys = Keys::generate();
                let pubkey = keys.public_key();

                let pubkey_tag = Tag::custom(TagKind::custom("n"), vec![pubkey.to_hex()]);

                // Create an announcement event builder
                let builder = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

                if let Err(e) = client.send_event_builder(builder).await {
                    log::error!("Failed to send device announcement: {}", e);
                } else {
                    log::info!("Device announcement sent");
                }

                Ok(Some(keys))
            }
        })
    }

    /// Initialize subscription for current user
    fn subscribe(current_user: PublicKey, cx: &AsyncApp) -> Task<Result<(), Error>> {
        cx.background_spawn(async move {
            let client = get_client();
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

            // Create a device announcement filter
            let device = Filter::new()
                .kind(Kind::Custom(DEVICE_ANNOUNCEMENT_KIND))
                .author(current_user)
                .limit(1);

            // Create a filter for getting all device keys activity
            let device_keys = Filter::new()
                .kinds(vec![
                    Kind::Custom(DEVICE_REQUEST_KIND),
                    Kind::Custom(DEVICE_RESPONSE_KIND),
                ])
                .author(current_user);

            // Create a contact list filter
            let contacts = Filter::new()
                .kind(Kind::ContactList)
                .author(current_user)
                .limit(1);

            // Create a user's data filter
            let data = Filter::new()
                .author(current_user)
                .since(Timestamp::now())
                .kinds(vec![
                    Kind::Metadata,
                    Kind::InboxRelays,
                    Kind::RelayList,
                    Kind::Custom(DEVICE_REQUEST_KIND),
                    Kind::Custom(DEVICE_RESPONSE_KIND),
                ]);

            // Create a filter for getting all gift wrapped events send to current user
            let msg = Filter::new().kind(Kind::GiftWrap).pubkey(current_user);

            // Create a filter to continuously receive new messages.
            let new_msg = Filter::new()
                .kind(Kind::GiftWrap)
                .pubkey(current_user)
                .limit(0);

            // Only subscribe to the latest device announcement
            client.subscribe(device, Some(opts)).await?;

            // Get all device keys activity (request/response)
            client.subscribe(device_keys, Some(opts)).await?;

            // Only subscribe to the latest contact list
            client.subscribe(contacts, Some(opts)).await?;

            // Continuously receive new user's data since now
            let sub_id = SubscriptionId::new(DATA_SUB_ID);
            client.subscribe_with_id(sub_id, data, None).await?;

            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            client.subscribe_with_id(sub_id, msg, Some(opts)).await?;

            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
            client.subscribe_with_id(sub_id, new_msg, None).await?;

            Ok(())
        })
    }
}
