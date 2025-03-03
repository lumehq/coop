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
use gpui::{App, AppContext, AsyncApp, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;

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
        match Device::init(public_key, &cx).await? {
            DeviceState::Master(keys) => {
                // Update global state with new device keys
                set_device_keys(keys.clone()).await;

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
/// NIP-4E: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
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

    /// Handle response event from other Nostr client
    pub fn handle_response(&self, event: &Event, cx: &Context<Self>) -> Task<Result<(), Error>> {
        let Some(local_keys) = self.local_keys.clone() else {
            return Task::ready(Err(anyhow!("Local keys not found")));
        };

        let local_signer = local_keys.into_nostr_signer();
        let target = event.tags.find(TagKind::custom("pubkey")).cloned();
        let content = event.content.clone();

        cx.background_spawn(async move {
            if let Some(tag) = target {
                let hex = tag.content().context(anyhow!("Public Key not found"))?;
                let public_key = PublicKey::parse(hex)?;

                let secret = local_signer.nip44_decrypt(&public_key, &content).await?;
                let keys = Keys::parse(&secret)?;

                log::info!("Received device keys from other client");
                // Update global state with new device keys
                set_device_keys(keys).await;

                Ok(())
            } else {
                Err(anyhow!("Failed to retrieve device key"))
            }
        })
    }

    /// Handle approve event for request from other Nostr client
    pub fn handle_request(&self, target: PublicKey, cx: &Context<Self>) -> Task<Result<(), Error>> {
        let client = get_client();
        let read_device_keys = cx.read_credentials(KEYRING);

        cx.background_spawn(async move {
            if let Ok(Some((_, secret))) = read_device_keys.await {
                let local_keys = Keys::generate();
                let local_pubkey = local_keys.public_key();
                let local_signer = local_keys.into_nostr_signer();

                // Get device's secret key
                let device_secret = String::from_utf8(secret)?;
                // Encrypt device's secret key by using NIP-44
                let content = local_signer.nip44_encrypt(&target, &device_secret).await?;

                // Create pubkey tag for other device (lowercase p)
                let other_tag = Tag::public_key(target);
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
        })
    }

    /// Initialize device's keys
    ///
    /// NIP-4E: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    fn init(current_user: PublicKey, cx: &AsyncApp) -> Task<Result<DeviceState, Error>> {
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
                println!("Event: {:?}", event);
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
                    log::error!("Failed to send device keys request: {}", e);
                } else {
                    log::info!("Announcement sent");
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

        // Create a filter to continuously receive device requests.
        let device_filter = Filter::new()
            .kinds(vec![
                Kind::Custom(DEVICE_REQUEST_KIND),
                Kind::Custom(DEVICE_RESPONSE_KIND),
            ])
            .author(current_user);

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

            // Continously receive new device requests
            client.subscribe(device_filter, None).await?;

            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            client.subscribe_with_id(sub_id, msg, Some(opts)).await?;

            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
            client.subscribe_with_id(sub_id, new_msg, None).await?;

            Ok(())
        })
    }
}
