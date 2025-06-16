use std::time::Duration;

use anyhow::{anyhow, Error};
use client_keys::ClientKeys;
use common::handle_auth::CoopAuthUrlHandler;
use global::{
    constants::{NIP17_RELAYS, NIP65_RELAYS, NOSTR_CONNECT_TIMEOUT},
    shared_state, NostrSignal,
};
use gpui::{
    div, App, AppContext, Context, Entity, Global, ParentElement, Styled, Subscription, Task,
    Window,
};
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use ui::{
    input::{InputState, TextInput},
    notification::Notification,
    ContextModal, Sizable,
};

pub fn init(window: &mut Window, cx: &mut App) {
    Identity::set_global(cx.new(|cx| Identity::new(window, cx)), cx);
}

struct GlobalIdentity(Entity<Identity>);

impl Global for GlobalIdentity {}

pub struct Identity {
    profile: Option<Profile>,
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
        let mut subscriptions = smallvec![];

        subscriptions.push(
            cx.observe_in(&client_keys, window, |this, state, window, cx| {
                let auto_login = AppSettings::get_global(cx).settings.auto_login;
                let has_client_keys = state.read(cx).has_keys();

                // Skip auto login if the user hasn't enabled auto login
                if has_client_keys && auto_login {
                    this.load(window, cx);
                } else {
                    this.set_profile(None, cx);
                }
            }),
        );

        Self {
            profile: None,
            subscriptions,
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier("coop:account")
                .limit(1);

            if let Some(event) = shared_state()
                .client
                .database()
                .query(filter)
                .await?
                .first_owned()
            {
                let secret = event.content;
                let is_bunker = secret.starts_with("bunker://");

                Ok((secret, is_bunker))
            } else {
                Err(anyhow!("Not found"))
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok((secret, is_bunker)) = task.await {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.login(&secret, is_bunker, window, cx);
                    })
                    .ok();
                })
                .ok();
            } else {
                this.update(cx, |this, cx| {
                    this.set_profile(None, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn login(
        &mut self,
        secret: &str,
        is_bunker: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_bunker {
            if let Ok(uri) = NostrConnectURI::parse(secret) {
                self.login_with_bunker(uri, window, cx);
            } else {
                self.set_profile(None, cx);
            }
        } else if let Ok(enc) = EncryptedSecretKey::from_bech32(secret) {
            self.login_with_keys(enc, window, cx);
        } else {
            self.set_profile(None, cx);
        }
    }

    pub(crate) fn login_with_bunker(
        &mut self,
        uri: NostrConnectURI,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT);
        let client_keys = ClientKeys::get_global(cx).keys();

        let Ok(mut signer) = NostrConnect::new(uri, client_keys, timeout, None) else {
            self.set_profile(None, cx);
            return;
        };
        // Automatically open auth url
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
                            Notification::error("Failed to connect to the remote signer"),
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

    pub(crate) fn login_with_keys(
        &mut self,
        enc: EncryptedSecretKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pwd_input = cx.new(|cx| InputState::new(window, cx).masked(true));
        let weak_input = pwd_input.downgrade();

        window.open_modal(cx, move |this, _window, _cx| {
            let weak_input = weak_input.clone();

            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .on_cancel(move |_, _window, cx| {
                    Identity::global(cx).update(cx, |this, cx| {
                        this.set_profile(None, cx);
                    });
                    true
                })
                .on_ok(move |_, window, cx| {
                    let value = weak_input
                        .read_with(cx, |state, _cx| state.value().to_string())
                        .ok();

                    if let Some(password) = value {
                        if password.is_empty() {
                            return false;
                        };

                        Identity::global(cx).update(cx, |this, cx| {
                            if let Ok(secret) = enc.decrypt(&password) {
                                this.set_signer(Keys::new(secret), window, cx);
                            } else {
                                this.set_profile(None, cx);
                            }
                        });
                    }

                    true
                })
                .child(
                    div()
                        .pt_4()
                        .px_4()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .child("Password to decrypt your key *")
                        .child(TextInput::new(&pwd_input).small()),
                )
        });
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
    pub fn new_identity(
        &mut self,
        keys: Keys,
        password: String,
        metadata: Metadata,
        cx: &mut Context<Self>,
    ) {
        let profile = Profile::new(keys.public_key(), metadata.clone());
        // Save keys for further use
        self.write_keys(&keys, password, cx);

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

    pub fn write_bunker(&self, uri: &NostrConnectURI, cx: &mut Context<Self>) {
        let mut value = uri.to_string();

        let Some(public_key) = uri.remote_signer_public_key().cloned() else {
            log::error!("Remote Signer's public key not found");
            return;
        };

        // Remove the secret param if it exists
        if let Some(secret) = uri.secret() {
            value = value.replace(secret, "");
        }

        cx.background_spawn(async move {
            let keys = Keys::generate();
            let builder = EventBuilder::new(Kind::ApplicationSpecificData, value).tags(vec![
                Tag::identifier("coop:account"),
                Tag::public_key(public_key),
            ]);

            if let Ok(event) = builder.sign(&keys).await {
                if let Err(e) = shared_state().client.database().save_event(&event).await {
                    log::error!("Failed to save event: {e}");
                };
            }
        })
        .detach();
    }

    pub fn write_keys(&self, keys: &Keys, password: String, cx: &mut Context<Self>) {
        let public_key = keys.public_key();

        if let Ok(enc_key) =
            EncryptedSecretKey::new(keys.secret_key(), &password, 16, KeySecurity::Medium)
        {
            cx.background_spawn(async move {
                let keys = Keys::generate();
                let builder =
                    EventBuilder::new(Kind::ApplicationSpecificData, enc_key.to_bech32().unwrap())
                        .tags(vec![
                            Tag::identifier("coop:account"),
                            Tag::public_key(public_key),
                        ]);

                if let Ok(event) = builder.sign(&keys).await {
                    if let Err(e) = shared_state().client.database().save_event(&event).await {
                        log::error!("Failed to save event: {e}");
                    };
                }
            })
            .detach();
        }
    }

    pub(crate) fn set_profile(&mut self, profile: Option<Profile>, cx: &mut Context<Self>) {
        self.profile = profile;
        cx.notify();
    }

    /// Returns the current profile
    pub fn profile(&self) -> Option<Profile> {
        self.profile.as_ref().cloned()
    }

    /// Returns true if a profile is currently loaded
    pub fn has_profile(&self) -> bool {
        self.profile.is_some()
    }
}
