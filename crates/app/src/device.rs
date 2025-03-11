use std::{collections::HashSet, str::FromStr, sync::Arc, time::Duration};

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

#[derive(Debug, Default)]
pub enum DeviceState {
    Master,
    Minion,
    #[default]
    None,
}

impl DeviceState {
    pub fn subscribe(&self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Device State: {:?}", self);
        match self {
            Self::Master => {
                let client = get_client();
                let task: Task<Result<(), Error>> = cx.background_spawn(async move {
                    let signer = client.signer().await?;
                    let public_key = signer.get_public_key().await?;

                    let opts =
                        SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

                    let filter = Filter::new()
                        .kind(Kind::Custom(DEVICE_REQUEST_KIND))
                        .author(public_key)
                        .limit(1);

                    // Subscribe for the latest request
                    client.subscribe(filter, Some(opts)).await?;

                    let filter = Filter::new()
                        .kind(Kind::Custom(DEVICE_REQUEST_KIND))
                        .author(public_key)
                        .since(Timestamp::now());

                    // Subscribe for new device requests
                    client.subscribe(filter, None).await?;

                    Ok(())
                });

                cx.spawn_in(window, |_, _cx| async move {
                    if let Err(err) = task.await {
                        log::error!("Failed to subscribe for device requests: {}", err);
                    }
                })
                .detach();
            }
            Self::Minion => {
                let client = get_client();
                let task: Task<Result<(), Error>> = cx.background_spawn(async move {
                    let signer = client.signer().await?;
                    let public_key = signer.get_public_key().await?;

                    let opts =
                        SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

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

                cx.spawn_in(window, |_, _cx| async move {
                    if let Err(err) = task.await {
                        log::error!("Failed to subscribe for device approval: {}", err);
                    }
                })
                .detach();
            }
            _ => {}
        }
    }
}

/// Current Device (Client)
///
/// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
#[derive(Debug)]
pub struct Device {
    /// Profile (Metadata) of current user
    profile: Option<NostrProfile>,
    /// Client Keys
    client_keys: Arc<Keys>,
    /// Device State
    state: Entity<DeviceState>,
    requesters: Entity<HashSet<PublicKey>>,
    is_processing: bool,
}

pub fn init(window: &mut Window, cx: &App) {
    // Initialize client keys
    let read_keys = cx.read_credentials(CLIENT_KEYRING);
    let window_handle = window.window_handle();

    cx.spawn(|cx| async move {
        let client_keys = if let Ok(Some((_, secret))) = read_keys.await {
            let secret_key = SecretKey::from_slice(&secret).unwrap();

            Arc::new(Keys::new(secret_key))
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

            Arc::new(keys)
        };

        cx.update(|cx| {
            let state = cx.new(|_| DeviceState::None);

            window_handle
                .update(cx, |_, window, cx| {
                    // Open the onboarding view
                    Root::update(window, cx, |this, window, cx| {
                        this.replace_view(onboarding::init(window, cx).into());
                        cx.notify();
                    });

                    window
                        .observe(&state, cx, |this, window, cx| {
                            this.update(cx, |this, cx| {
                                this.subscribe(window, cx);
                            });
                        })
                        .detach();
                })
                .ok();

            let requesters = cx.new(|_| HashSet::new());
            let entity = cx.new(|_| Device {
                profile: None,
                is_processing: false,
                state,
                client_keys,
                requesters,
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

    pub fn set_state(&mut self, state: DeviceState, cx: &mut Context<Self>) {
        self.state.update(cx, |this, cx| {
            *this = state;
            cx.notify();
        });
    }

    pub fn set_processing(&mut self, is_processing: bool, cx: &mut Context<Self>) {
        self.is_processing = is_processing;
        cx.notify();
    }

    pub fn add_requester(&mut self, public_key: PublicKey, cx: &mut Context<Self>) {
        self.requesters.update(cx, |this, cx| {
            this.insert(public_key);
            cx.notify();
        });
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
                .await?
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
        if self.is_processing {
            return;
        }

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

    /// Setup Device
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn setup_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self.profile().cloned() else {
            return;
        };

        // If processing, return early
        if self.is_processing {
            return;
        }

        // Only process if device keys are not set
        self.set_processing(true, cx);

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
            // Device Keys has been set, no need to retrieve device announcement again
            if get_device_keys().await.is_some() {
                return;
            }

            match fetch_announcement.await {
                Ok(event) => {
                    log::info!("Found a device announcement: {:?}", event);

                    let n_tag = event
                        .tags
                        .find(TagKind::custom("n"))
                        .and_then(|t| t.content())
                        .map(|hex| hex.to_owned());

                    let credentials_task =
                        match cx.update(|_, cx| cx.read_credentials(MASTER_KEYRING)) {
                            Ok(task) => task,
                            Err(err) => {
                                log::error!("Failed to read credentials: {:?}", err);
                                log::info!("Trying to request keys from Master Device...");

                                cx.update(|window, cx| {
                                    this.update(cx, |this, cx| {
                                        this.request_master_keys(window, cx);
                                    })
                                })
                                .ok();

                                return;
                            }
                        };

                    match credentials_task.await {
                        Ok(Some((pubkey, secret))) if n_tag.as_deref() == Some(&pubkey) => {
                            cx.update(|window, cx| {
                                this.update(cx, |this, cx| {
                                    this.set_master_keys(secret, window, cx);
                                })
                            })
                            .ok();
                        }
                        _ => {
                            log::info!("This device is not the Master Device.");
                            log::info!("Trying to request keys from Master Device...");

                            cx.update(|window, cx| {
                                this.update(cx, |this, cx| {
                                    this.request_master_keys(window, cx);
                                })
                            })
                            .ok();
                        }
                    }
                }
                Err(_) => {
                    log::info!("Device Announcement not found.");
                    log::info!("Appoint this device as Master Device.");

                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_new_master_keys(window, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Create a new Master Keys, appointing this device as Master Device.
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn set_new_master_keys(&self, window: &mut Window, cx: &Context<Self>) {
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

        cx.spawn_in(window, |this, mut cx| async move {
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

                cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        this.set_state(DeviceState::Master, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Device already has Master Keys, re-appointing this device as Master Device.
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn set_master_keys(&self, secret: Vec<u8>, window: &mut Window, cx: &Context<Self>) {
        let Ok(secret_key) = SecretKey::from_slice(&secret) else {
            log::error!("Failed to parse secret key");
            return;
        };
        let keys = Arc::new(Keys::new(secret_key));

        cx.spawn_in(window, |this, mut cx| async move {
            log::info!("Re-appointing this device as Master Device.");
            set_device_keys(keys).await;

            cx.update(|_, cx| {
                this.update(cx, |this, cx| {
                    this.set_state(DeviceState::Master, cx);
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    /// Send a request to ask for device keys from the other Nostr client
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn request_master_keys(&self, window: &mut Window, cx: &Context<Self>) {
        let client = get_client();
        let app_name = get_app_name();
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

            Ok(())
        });

        cx.spawn_in(window, |this, mut cx| async move {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_state(DeviceState::Minion, cx);
                        this.render_waiting_modal(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Received Device Keys approval from Master Device,
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn recv_approval(&self, event: Event, window: &mut Window, cx: &Context<Self>) {
        let local_signer = self.client_keys.clone();

        let task = cx.background_spawn(async move {
            if let Some(tag) = event
                .tags
                .find(TagKind::custom("P"))
                .and_then(|tag| tag.content())
            {
                if let Ok(public_key) = PublicKey::from_str(tag) {
                    let secret = local_signer
                        .nip44_decrypt(&public_key, &event.content)
                        .await?;

                    let keys = Arc::new(Keys::parse(&secret)?);

                    // Update global state with new device keys
                    set_device_keys(keys).await;
                    log::info!("Received master keys");

                    Ok(())
                } else {
                    Err(anyhow!("Public Key is invalid"))
                }
            } else {
                Err(anyhow!("Failed to decrypt the Master Keys"))
            }
        });

        cx.spawn_in(window, |_, mut cx| async move {
            // No need to update if device keys are already available
            if get_device_keys().await.is_some() {
                return;
            }

            if let Err(e) = task.await {
                cx.update(|window, cx| {
                    window.push_notification(
                        Notification::error(format!("Failed to decrypt: {}", e)),
                        cx,
                    );
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    window.close_all_modals(cx);
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

    /// Received Master Keys request from other Nostr client
    ///
    /// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    pub fn recv_request(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target_pubkey) = event
            .tags
            .find(TagKind::custom("pubkey"))
            .and_then(|tag| tag.content())
            .and_then(|content| PublicKey::parse(content).ok())
        else {
            log::error!("Invalid public key.");
            return;
        };

        // Prevent processing duplicate requests
        if self.requesters.read(cx).contains(&target_pubkey) {
            return;
        }

        self.add_requester(target_pubkey, cx);

        let client = get_client();
        let read_keys = cx.read_credentials(MASTER_KEYRING);
        let local_signer = self.client_keys.clone();

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
                        let device_secret_hex = device_secret.to_secret_hex();

                        // Encrypt device's secret key by using NIP-44
                        let content = local_signer
                            .nip44_encrypt(&target_pubkey, &device_secret_hex)
                            .await?;

                        // Create pubkey tag for other device (lowercase p)
                        let other_tag = Tag::public_key(target_pubkey);

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
                .width(px(430.))
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
                .width(px(430.))
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
