use std::time::Duration;

use anyhow::Error;
use client_keys::ClientKeys;
use common::handle_auth::CoopAuthUrlHandler;
use global::{
    constants::{
        KEYRING_BUNKER, KEYRING_USER_PATH, NIP17_RELAYS, NIP65_RELAYS, NOSTR_CONNECT_TIMEOUT,
    },
    shared_state, NostrSignal,
};
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task, Window};
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use ui::{notification::Notification, ContextModal};

pub fn init(window: &mut Window, cx: &mut App) {
    Identity::set_global(cx.new(|cx| Identity::new(window, cx)), cx);
}

struct GlobalIdentity(Entity<Identity>);

impl Global for GlobalIdentity {}

pub struct Identity {
    profile: Entity<Option<Profile>>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Identity {
    /// Retrieve the Global Identity instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalIdentity>().0.clone()
    }

    /// Retrieve the Identity instance
    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalIdentity>().0.read(cx)
    }

    /// Set the Global Identity instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalIdentity(state));
    }

    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client_keys = ClientKeys::global(cx);
        let profile = cx.new(|_| None);
        let mut subscriptions = smallvec![];

        subscriptions.push(
            cx.observe_in(&client_keys, window, |this, state, window, cx| {
                let auto_login = AppSettings::get_global(cx).settings().auto_login;
                let has_client_keys = state.read(cx).has_keys(cx);

                // Skip auto login if the user hasn't enabled auto login
                if has_client_keys && auto_login {
                    this.load(window, cx);
                } else {
                    this.set_profile(None, cx);
                }
            }),
        );

        Self {
            profile,
            subscriptions,
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read_user_keys = cx.read_credentials(KEYRING_USER_PATH);

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((username, secret))) = read_user_keys.await else {
                // User cancelled the action or failed to read credentials, then notify UI
                this.update(cx, |this, cx| {
                    this.set_profile(None, cx);
                })
                .ok();

                return;
            };

            // Process to login with saved user credentials
            cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.auto_login(username, secret, window, cx);
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn auto_login(
        &self,
        username: String,
        secret: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if username == KEYRING_BUNKER {
            // Process to login with nostr connect
            match secret_to_bunker(secret) {
                Ok(uri) => {
                    let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT);
                    let client_keys = ClientKeys::get_global(cx).keys(cx);

                    match NostrConnect::new(uri, client_keys, timeout, None) {
                        Ok(mut signer) => {
                            // Automatically open remote signer's webpage when received auth url
                            signer.auth_url_handler(CoopAuthUrlHandler);

                            let (tx, rx) = oneshot::channel::<Option<NostrConnect>>();

                            // Verify the signer, make sure Remote Signer is connected
                            cx.background_spawn(async move {
                                if signer.bunker_uri().await.is_ok() {
                                    tx.send(Some(signer)).ok();
                                } else {
                                    tx.send(None).ok();
                                }
                            })
                            .detach();

                            cx.spawn_in(window, async move |this, cx| {
                                match rx.await {
                                    Ok(Some(signer)) => {
                                        cx.update(|window, cx| {
                                            this.update(cx, |this, cx| {
                                                this.set_signer(signer, window, cx);
                                            })
                                            .ok();
                                        })
                                        .ok();
                                    }
                                    _ => {
                                        cx.update(|window, cx| {
                                            window.push_notification(
                                                Notification::error(
                                                    "Failed to connect to the remote signer",
                                                ),
                                                cx,
                                            );
                                            this.update(cx, |this, cx| {
                                                this.set_profile(None, cx);
                                            })
                                            .ok();
                                        })
                                        .ok();
                                    }
                                };
                            })
                            .detach();
                        }
                        Err(_) => self.set_profile(None, cx),
                    }
                }
                Err(_) => self.set_profile(None, cx),
            }
        } else {
            // Process to login with secret key
            match SecretKey::from_slice(&secret) {
                Ok(secret_key) => self.set_signer(Keys::new(secret_key), window, cx),
                Err(_) => self.set_profile(None, cx),
            }
        }
    }

    /// Sets a new signer for the client and updates user identity
    pub fn set_signer<S>(&self, signer: S, window: &mut Window, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        let task: Task<Result<Profile, Error>> = cx.background_spawn(async move {
            let public_key = signer.get_public_key().await?;

            // Update signer
            shared_state().client.set_signer(signer).await;

            // Fetch user's metadata
            let metadata = shared_state()
                .client
                .fetch_metadata(public_key, Duration::from_secs(2))
                .await?
                .unwrap_or_default();

            // Create user's profile with public key and metadata
            let profile = Profile::new(public_key, metadata);

            // Subscribe for user's data
            nostr_sdk::async_utility::task::spawn(async move {
                shared_state().subscribe_for_user_data(public_key).await;
            });

            // Notify GPUi via the global channel
            shared_state()
                .global_sender
                .send(NostrSignal::SignerUpdated)
                .await?;

            Ok(profile)
        });

        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(profile) => {
                this.update(cx, |this, cx| {
                    this.set_profile(Some(profile), cx);
                })
                .ok();
            }
            Err(e) => {
                cx.update(|window, cx| {
                    window.push_notification(Notification::error(e.to_string()), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// Creates a new identity with the given keys and metadata
    pub fn new_identity(&mut self, keys: Keys, metadata: Metadata, cx: &mut Context<Self>) {
        let profile = Profile::new(keys.public_key(), metadata.clone());
        let save = cx.write_credentials(
            KEYRING_USER_PATH,
            keys.public_key().to_hex().as_str(),
            keys.secret_key().as_secret_bytes(),
        );

        cx.background_spawn(async move {
            if let Err(e) = save.await {
                log::error!("Failed to save keys: {}", e)
            };
        })
        .detach();

        cx.background_spawn(async move {
            // Update signer
            shared_state().client.set_signer(keys).await;
            // Set metadata
            shared_state().client.set_metadata(&metadata).await.ok();

            // Create relay list
            let builder = EventBuilder::new(Kind::RelayList, "").tags(
                NIP65_RELAYS.into_iter().filter_map(|url| {
                    if let Ok(url) = RelayUrl::parse(url) {
                        Some(Tag::relay_metadata(url, None))
                    } else {
                        None
                    }
                }),
            );

            if let Err(e) = shared_state().client.send_event_builder(builder).await {
                log::error!("Failed to send relay list event: {}", e);
            };

            // Create messaging relay list
            let builder = EventBuilder::new(Kind::InboxRelays, "").tags(
                NIP17_RELAYS.into_iter().filter_map(|url| {
                    if let Ok(url) = RelayUrl::parse(url) {
                        Some(Tag::relay(url))
                    } else {
                        None
                    }
                }),
            );

            if let Err(e) = shared_state().client.send_event_builder(builder).await {
                log::error!("Failed to send messaging relay list event: {}", e);
            };

            // Notify GPUi via the global channel
            shared_state()
                .global_sender
                .send(NostrSignal::SignerUpdated)
                .await
                .ok();

            // Subscribe
            shared_state()
                .subscribe_for_user_data(profile.public_key())
                .await;
        })
        .detach();
    }

    pub fn save_bunker_uri(&self, uri: &NostrConnectURI, cx: &mut Context<Self>) {
        let mut value = uri.to_string();

        // Remove the secret param if it exists
        if let Some(secret) = uri.secret() {
            value = value.replace(secret, "");
        }

        let save_bunker_uri =
            cx.write_credentials(KEYRING_USER_PATH, KEYRING_BUNKER, value.as_bytes());

        cx.background_spawn(async move {
            if let Err(e) = save_bunker_uri.await {
                log::error!("Failed to save the Bunker URI: {}", e)
            }
        })
        .detach();
    }

    pub fn save_keys(&self, keys: &Keys, cx: &mut Context<Self>) {
        let save_credential = cx.write_credentials(
            KEYRING_USER_PATH,
            keys.public_key().to_hex().as_str(),
            keys.secret_key().as_secret_bytes(),
        );

        cx.background_spawn(async move {
            if let Err(e) = save_credential.await {
                log::error!("Failed to save keys: {}", e)
            }
        })
        .detach();
    }

    pub(crate) fn set_profile(&self, profile: Option<Profile>, cx: &mut Context<Self>) {
        self.profile.update(cx, |this, cx| {
            *this = profile;
            cx.notify();
        });
    }

    /// Returns the current profile
    pub fn profile(&self, cx: &App) -> Option<Profile> {
        self.profile.read(cx).as_ref().cloned()
    }

    /// Returns true if a profile is currently loaded
    pub fn has_profile(&self, cx: &App) -> bool {
        self.profile.read(cx).is_some()
    }
}

fn secret_to_bunker(secret: Vec<u8>) -> Result<NostrConnectURI, Error> {
    let value = String::from_utf8(secret)?;
    let uri = NostrConnectURI::parse(value)?;

    Ok(uri)
}
