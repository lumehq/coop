use std::time::Duration;

use anyhow::{anyhow, Context as AnyContext, Error};
use common::profile::NostrProfile;
use global::{
    constants::{
        ALL_MESSAGES_SUB_ID, DEVICE_ANNOUNCEMENT_KIND, DEVICE_REQUEST_KIND, DEVICE_RESPONSE_KIND,
        KEYRING, NEW_MESSAGE_SUB_ID,
    },
    get_app_name, get_client, set_device_keys,
};
use gpui::{App, AppContext, AsyncApp, Context, Entity, Global, Task, Window};
use nostr_sdk::prelude::*;
use ui::{notification::Notification, ContextModal};

pub fn init<T>(user_signer: T, cx: &AsyncApp) -> Task<Result<(), Error>>
where
    T: NostrSigner + 'static,
{
    let client = get_client();

    // Set the user's signer as the main signer
    let set_signer: Task<Result<NostrProfile, Error>> = cx.background_spawn(async move {
        // Use user's signer for main signer
        _ = client.set_signer(user_signer).await;

        // Verify nostr signer and get public key
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        // Fetch user's metadata
        let metadata = client
            .fetch_metadata(public_key, Duration::from_secs(2))
            .await
            .unwrap_or_default();

        Ok(NostrProfile::new(public_key, metadata))
    });

    cx.spawn(|cx| async move {
        let profile = set_signer.await?;
        let public_key = profile.public_key();

        // Run initial subscription for this user
        Device::subscribe(public_key, &cx).await?;

        // Initialize device
        match Device::setup_device(public_key, &cx).await? {
            DeviceState::Master(keys) => {
                // Update global state with new device keys
                set_device_keys(keys.clone()).await;

                // Subscribe to device keys requests
                cx.background_spawn(async move {
                    log::info!("Subscribing to device keys requests...");

                    let filter = Filter::new()
                        .kind(Kind::Custom(DEVICE_REQUEST_KIND))
                        .author(public_key)
                        .since(Timestamp::now());

                    _ = client.subscribe(filter, None).await;
                })
                .await;

                _ = cx.update(|cx| {
                    // Save device keys to keyring for future use
                    let password = keys.secret_key().as_secret_bytes();
                    let app_name = get_app_name();
                    let task = cx.write_credentials(KEYRING, app_name, password);

                    cx.spawn(|_cx| async move {
                        if let Err(e) = task.await {
                            log::error!("Failed to save device keys to keyring: {}", e);
                        }
                    })
                    .detach();

                    let device = cx.new(|_| Device {
                        profile,
                        local_keys: None,
                    });

                    Device::set_global(device, cx);
                });
            }
            DeviceState::Minion(local_keys) => {
                _ = cx.update(|cx| {
                    let device = cx.new(|_| Device {
                        profile,
                        local_keys: Some(local_keys),
                    });

                    Device::set_global(device, cx);
                })
            }
        }

        Ok(())
    })
}

struct GlobalDevice(Entity<Device>);

impl Global for GlobalDevice {}

#[derive(Debug)]
enum DeviceState {
    Master(Keys),
    Minion(Keys),
}

/// Current Device (Client)
///
/// NIP-4e: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
#[derive(Debug)]
pub struct Device {
    /// Profile (Metadata) of current user
    profile: NostrProfile,
    /// Local Keys, used for requesting device keys from others
    local_keys: Option<Keys>,
}

impl Device {
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalDevice>().map(|model| model.0.clone())
    }

    pub fn set_global(device: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalDevice(device));
    }

    /// Get the account's profile
    pub fn profile(&self) -> &NostrProfile {
        &self.profile
    }

    /// Create a task to verify inbox relays
    pub fn verify_inbox_relays(&self, cx: &App) -> Task<Result<Vec<String>, Error>> {
        let client = get_client();
        let public_key = self.profile.public_key();

        cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            // Get inbox relays event from database
            let events = client.database().query(filter).await?;

            if let Some(event) = events.first_owned() {
                let relays = event
                    .tags
                    .filter_standardized(TagKind::Relay)
                    .filter_map(|t| {
                        if let TagStandard::Relay(url) = t {
                            Some(url.to_string())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                Ok(relays)
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    /// Receive device keys approval from other Nostr client,
    /// then process and update device keys.
    pub fn handle_response(&self, event: Event, window: &mut Window, cx: &Context<Self>) {
        let Some(local_keys) = self.local_keys.clone() else {
            return;
        };

        let handle = window.window_handle();
        let local_signer = local_keys.into_nostr_signer();

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
        let read_device_keys = cx.read_credentials(KEYRING);

        let approve_task = cx.background_spawn(async move {
            if let Ok(Some((_, secret))) = read_device_keys.await {
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

    /// Initialize device's keys
    ///
    /// NIP-4E: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    fn setup_device(current_user: PublicKey, cx: &AsyncApp) -> Task<Result<DeviceState, Error>> {
        // Create a task to get device keys from keyring
        let Ok(read_keys) = cx.update(|cx| cx.read_credentials(KEYRING)) else {
            return Task::ready(Err(anyhow!("Failed to read device keys from keyring")));
        };

        let client = get_client();
        let app_name = get_app_name();

        // Create a task to verify device keys
        cx.background_spawn(async move {
            let kind = Kind::Custom(DEVICE_ANNOUNCEMENT_KIND);
            let filter = Filter::new().kind(kind).author(current_user).limit(1);

            // Fetch device announcement events
            let events = client.database().query(filter).await?;

            // Found device announcement event,
            // that means user is already used another Nostr client or re-install this client
            if let Some(event) = events.first_owned() {
                // Check if device keys are found in keyring
                if let Ok(Some((_, secret))) = read_keys.await {
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
                        return Ok(DeviceState::Master(keys));
                    }
                    // Otherwise fall through to request device keys
                }

                // Send a request for device keys to the other Nostr client
                //
                // Create a local keys to send request
                let keys = Keys::generate();
                let pubkey = keys.public_key();

                let kind = Kind::Custom(DEVICE_REQUEST_KIND);
                let client_tag = Tag::client(app_name);
                let pubkey_tag = Tag::custom(TagKind::custom("pubkey"), vec![pubkey.to_hex()]);
                // Create a request event builder
                let builder = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

                if let Err(e) = client.send_event_builder(builder).await {
                    log::error!("Failed to send device keys request: {}", e);
                }

                log::info!("Sent a request to ask for device keys from the other Nostr client");
                log::info!("Waiting for response...");

                let resp = Filter::new()
                    .kind(Kind::Custom(DEVICE_RESPONSE_KIND))
                    .author(current_user)
                    .since(Timestamp::now());

                // Continously receive the request approval
                client.subscribe(resp, None).await?;

                Ok(DeviceState::Minion(keys))
            } else {
                log::info!("Device announcement is not found, appoint this device as master");

                let keys = Keys::generate();
                let pubkey = keys.public_key();

                let client_tag = Tag::client(app_name);
                let pubkey_tag = Tag::custom(TagKind::custom("n"), vec![pubkey.to_hex()]);
                // Create an announcement event builder
                let builder = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

                if let Err(e) = client.send_event_builder(builder).await {
                    log::error!("Failed to send device announcement: {}", e);
                } else {
                    log::info!("Device announcement sent");
                }

                Ok(DeviceState::Master(keys))
            }
        })
    }

    /// Run initial subscription
    fn subscribe(current_user: PublicKey, cx: &AsyncApp) -> Task<Result<(), Error>> {
        let client = get_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        // Create a device announcement filter
        let device = Filter::new()
            .kind(Kind::Custom(DEVICE_ANNOUNCEMENT_KIND))
            .author(current_user)
            .limit(1);

        // Create a contact list filter
        let contacts = Filter::new()
            .kind(Kind::ContactList)
            .author(current_user)
            .limit(1);

        // Create a user's data filter
        let data = Filter::new()
            .author(current_user)
            .since(Timestamp::now())
            .kinds(vec![Kind::Metadata, Kind::InboxRelays, Kind::RelayList]);

        // Create a filter for getting all gift wrapped events send to current user
        let msg = Filter::new().kind(Kind::GiftWrap).pubkey(current_user);

        // Create a filter to continuously receive new messages.
        let new_msg = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(current_user)
            .limit(0);

        cx.background_spawn(async move {
            // Only subscribe to the latest device announcement
            client.subscribe(device, Some(opts)).await?;

            // Only subscribe to the latest contact list
            client.subscribe(contacts, Some(opts)).await?;

            // Continuously receive new user's data since now
            client.subscribe(data, None).await?;

            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            client.subscribe_with_id(sub_id, msg, Some(opts)).await?;

            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
            client.subscribe_with_id(sub_id, new_msg, None).await?;

            Ok(())
        })
    }
}
