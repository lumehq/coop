use anyhow::{anyhow, Context as AnyContext, Error};
use common::{
    constants::{ALL_MESSAGES_SUB_ID, NEW_MESSAGE_SUB_ID},
    profile::NostrProfile,
};
use constants::*;
use gpui::{App, AppContext, AsyncApp, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use state::get_client;
use std::{sync::Arc, time::Duration};

pub mod constants;

pub fn init(user_signer: Arc<dyn NostrSigner>, cx: &AsyncApp) -> Task<Result<(), Error>> {
    let client = get_client();
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

        cx.update(|cx| {
            let this = cx.new(|cx| {
                let this = Device {
                    profile,
                    master_signer: None,
                    local_keys: None,
                };
                // Run initial setup for this account
                this.setup(cx);
                // Initialize device's keys
                this.init_device(cx);

                this
            });

            Device::set_global(this, cx)
        })
    })
}

struct GlobalDevice(Entity<Device>);

impl Global for GlobalDevice {}

/// Current Device (Client)
///
/// NIP-4E: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
#[derive(Debug)]
pub struct Device {
    /// Profile (Metadata) of current user
    profile: NostrProfile,
    /// Master Signer, this can be created by this device or requested from others
    master_signer: Option<Arc<dyn NostrSigner>>,
    /// Local Keys, used for requesting master keys from others
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

    /// Initialize device's keys
    ///
    /// NIP-4E: <https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md>
    fn init_device(&self, cx: &Context<Self>) {
        let client = get_client();
        let public_key = self.profile().public_key();

        let task: Task<Result<Keys, Error>> = cx.background_spawn(async move {
            let kind = Kind::Custom(DEVICE_ANNOUNCEMENT_KIND);
            let filter = Filter::new().kind(kind).author(public_key).limit(1);

            // Fetch device announcement events
            let events = client.fetch_events(filter, Duration::from_secs(2)).await?;

            // If device announcement events exists,
            // this client does not need to create device keys,
            // instead, it sends a request to the other client to ask for master keys.
            if events.first_owned().is_some() {
                // Create a temporary local keys, only use for master keys request
                // THIS IS NOT MASTER KEYS
                let keys = Keys::generate();

                let kind = Kind::Custom(DEVICE_REQUEST_KIND);
                let app_name = get_app_name();
                let client_tag = Tag::client(app_name);
                let pubkey_tag =
                    Tag::custom(TagKind::custom("pubkey"), vec![keys.public_key().to_hex()]);

                // Create request device keys event builder
                let builder = EventBuilder::new(kind, "").tags(vec![client_tag, pubkey_tag]);

                if let Err(e) = client.send_event_builder(builder).await {
                    log::error!("Failed to send device keys request: {}", e);
                }

                Ok(keys)
            } else {
                // Create a new device keys
                let keys = Keys::generate();

                Ok(keys)
            }
        });

        cx.spawn(|this, cx| async move {
            if let Ok(keys) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.local_keys = Some(keys);
                        cx.notify();
                    })
                    .ok()
                })
                .ok();
            }
        })
        .detach();
    }

    /// Handle response event from other Nostr client
    pub fn handle_response(&self, event: &Event, cx: &Context<Self>) {
        let Some(local_keys) = self.local_keys.clone() else {
            return;
        };

        let local_signer = local_keys.into_nostr_signer();
        let target = event.tags.find(TagKind::custom("pubkey")).cloned();
        let content = event.content.clone();

        let task: Task<Result<Keys, Error>> = cx.background_spawn(async move {
            if let Some(tag) = target {
                let hex = tag.content().context(anyhow!("Public Key not found"))?;
                let public_key = PublicKey::parse(hex)?;

                let secret = local_signer.nip44_decrypt(&public_key, &content).await?;
                let keys = Keys::parse(&secret)?;

                Ok(keys)
            } else {
                Err(anyhow!("Failed to retrieve master key"))
            }
        });

        cx.spawn(|this, cx| async move {
            if let Ok(keys) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.master_signer = Some(Arc::new(keys));
                        cx.notify();
                    })
                    .ok()
                })
                .ok();
            }
        })
        .detach();
    }

    /// Handle approve event for request from other Nostr client
    pub fn handle_request(&self, target: PublicKey, cx: &Context<Self>) {
        let client = get_client();
        let read_device_keys = cx.read_credentials(DEVICE_KEYRING);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
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

                Ok(())
            } else {
                Err(anyhow!("Device Keys not found"))
            }
        });

        task.detach();
    }

    /// Run initial setup for logging in account
    fn setup(&self, cx: &mut Context<Self>) {
        let client = get_client();
        let public_key = self.profile.public_key();

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

            // Get contact list
            let contact_list = Filter::new()
                .kind(Kind::ContactList)
                .author(public_key)
                .limit(1);

            client.subscribe(contact_list, Some(opts)).await?;

            // Create a filter to continuously receive new user's data.
            let data = Filter::new()
                .kinds(vec![Kind::Metadata, Kind::InboxRelays, Kind::RelayList])
                .author(public_key)
                .since(Timestamp::now());

            client.subscribe(data, None).await?;

            // Create a filter for getting all gift wrapped events send to current user
            let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
            let sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);

            client
                .subscribe_with_id(sub_id, filter.clone(), Some(opts))
                .await?;

            // Create a filter to continuously receive new messages.
            let new_filter = filter.limit(0);
            let sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

            client.subscribe_with_id(sub_id, new_filter, None).await?;

            // Create a filter to continuously receive device requests.
            let device_filter = Filter::new()
                .kinds(vec![
                    Kind::Custom(DEVICE_REQUEST_KIND),
                    Kind::Custom(DEVICE_RESPONSE_KIND),
                ])
                .author(public_key);

            client.subscribe(device_filter, None).await?;

            Ok(())
        });

        task.detach();
    }
}
