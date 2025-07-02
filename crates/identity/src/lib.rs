use std::time::Duration;

use anyhow::{anyhow, Error};
use client_keys::ClientKeys;
use common::handle_auth::CoopAuthUrlHandler;
use global::constants::{ACCOUNT_D, NIP17_RELAYS, NIP65_RELAYS, NOSTR_CONNECT_TIMEOUT};
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, red, App, AppContext, Context, Entity, Global, ParentElement, SharedString, Styled,
    Subscription, Task, WeakEntity, Window,
};
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use ui::input::{InputState, TextInput};
use ui::notification::Notification;
use ui::{ContextModal, Sizable};

pub fn init(window: &mut Window, cx: &mut App) {
    Identity::set_global(cx.new(|cx| Identity::new(window, cx)), cx);
}

struct GlobalIdentity(Entity<Identity>);

impl Global for GlobalIdentity {}

pub struct Identity {
    profile: Option<Profile>,
    auto_logging_in_progress: bool,
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
                    this.set_logging_in(true, cx);
                    this.load(window, cx);
                } else {
                    this.set_profile(None, cx);
                }
            }),
        );

        Self {
            profile: None,
            auto_logging_in_progress: false,
            subscriptions,
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let database = shared_state().client().database();

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_D)
                .limit(1);

            if let Some(event) = database.query(filter).await?.first_owned() {
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

    pub fn unload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let client = shared_state().client();
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_D)
                .limit(1);

            // Unset signer
            client.unset_signer().await;

            // Delete account
            client.database().delete(filter).await.is_ok()
        });

        cx.spawn_in(window, async move |this, cx| {
            if task.await {
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
                window.push_notification(Notification::error("Bunker URI is invalid"), cx);
                self.set_profile(None, cx);
            }
        } else if let Ok(enc) = EncryptedSecretKey::from_bech32(secret) {
            self.login_with_keys(enc, window, cx);
        } else {
            window.push_notification(Notification::error("Secret Key is invalid"), cx);
            self.set_profile(None, cx);
        }
    }

    pub(crate) fn login_with_bunker(
        &mut self,
        uri: NostrConnectURI,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT / 10);
        let client_keys = ClientKeys::get_global(cx).keys();

        let Ok(mut signer) = NostrConnect::new(uri, client_keys, timeout, None) else {
            window.push_notification(
                Notification::error("Bunker URI is invalid").title("Nostr Connect"),
                cx,
            );
            self.set_profile(None, cx);
            return;
        };
        // Automatically open auth url
        signer.auth_url_handler(CoopAuthUrlHandler);

        cx.spawn_in(window, async move |this, cx| {
            // Call .bunker_uri() to verify the connection
            match signer.bunker_uri().await {
                Ok(_) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_signer(signer, window, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(e.to_string()).title("Nostr Connect"),
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
        let pwd_input: Entity<InputState> = cx.new(|cx| InputState::new(window, cx).masked(true));
        let weak_input = pwd_input.downgrade();

        let error: Entity<Option<SharedString>> = cx.new(|_| None);
        let weak_error = error.downgrade();

        let entity = cx.weak_entity();

        window.open_modal(cx, move |this, _window, cx| {
            let entity = entity.clone();
            let entity_clone = entity.clone();
            let weak_input = weak_input.clone();
            let weak_error = weak_error.clone();

            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .on_cancel(move |_, _window, cx| {
                    entity
                        .update(cx, |this, cx| {
                            this.set_profile(None, cx);
                        })
                        .ok();
                    // Close modal
                    true
                })
                .on_ok(move |_, window, cx| {
                    let weak_error = weak_error.clone();
                    let password = weak_input
                        .read_with(cx, |state, _cx| state.value().to_owned())
                        .ok();

                    entity_clone
                        .update(cx, |this, cx| {
                            this.verify_keys(enc, password, weak_error, window, cx);
                        })
                        .ok();

                    false
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
                        .child(TextInput::new(&pwd_input).small())
                        .when_some(error.read(cx).as_ref(), |this, error| {
                            this.child(
                                div()
                                    .text_xs()
                                    .italic()
                                    .text_color(red())
                                    .child(error.clone()),
                            )
                        }),
                )
        });
    }

    pub(crate) fn verify_keys(
        &mut self,
        enc: EncryptedSecretKey,
        password: Option<SharedString>,
        error: WeakEntity<Option<SharedString>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(password) = password else {
            _ = error.update(cx, |this, cx| {
                *this = Some("Password is required".into());
                cx.notify();
            });
            return;
        };

        if password.is_empty() {
            _ = error.update(cx, |this, cx| {
                *this = Some("Password cannot be empty".into());
                cx.notify();
            });
            return;
        }

        // Decrypt the password in the background to prevent blocking the main thread
        let task: Task<Option<SecretKey>> =
            cx.background_spawn(async move { enc.decrypt(&password).ok() });

        cx.spawn_in(window, async move |this, cx| {
            if let Some(secret) = task.await {
                cx.update(|window, cx| {
                    window.close_modal(cx);
                    // Update user's signer with decrypted secret key
                    this.update(cx, |this, cx| {
                        this.set_signer(Keys::new(secret), window, cx);
                    })
                    .ok();
                })
                .ok();
            } else {
                _ = error.update(cx, |this, cx| {
                    *this = Some("Invalid password".into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Sets a new signer for the client and updates user identity
    pub fn set_signer<S>(&self, signer: S, window: &mut Window, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        let task: Task<Result<Profile, Error>> = cx.background_spawn(async move {
            let client = shared_state().client();
            let public_key = signer.get_public_key().await?;

            // Update signer
            client.set_signer(signer).await;

            // Subscribe for user's data
            shared_state().subscribe_for_user_data(public_key).await;

            // Fetch user's metadata
            let metadata = client
                .fetch_metadata(public_key, Duration::from_secs(3))
                .await?
                .unwrap_or_default();

            // Create user's profile with public key and metadata
            Ok(Profile::new(public_key, metadata))
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
            let client = shared_state().client();

            // Update signer
            client.set_signer(keys).await;
            // Set metadata
            client.set_metadata(&metadata).await.ok();

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

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send relay list event: {e}");
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

            if let Err(e) = client.send_event_builder(builder).await {
                log::error!("Failed to send messaging relay list event: {e}");
            };

            // Subscribe for user's data
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
            let client = shared_state().client();
            let keys = Keys::generate();

            let builder = EventBuilder::new(Kind::ApplicationSpecificData, value).tags(vec![
                Tag::identifier(ACCOUNT_D),
                Tag::public_key(public_key),
            ]);

            if let Ok(event) = builder.sign(&keys).await {
                if let Err(e) = client.database().save_event(&event).await {
                    log::error!("Failed to save event: {e}");
                };
            }
        })
        .detach();
    }

    pub fn write_keys(&self, keys: &Keys, password: String, cx: &mut Context<Self>) {
        let keys = keys.to_owned();
        let public_key = keys.public_key();

        cx.background_spawn(async move {
            if let Ok(enc_key) =
                EncryptedSecretKey::new(keys.secret_key(), &password, 8, KeySecurity::Unknown)
            {
                let client = shared_state().client();
                let keys = Keys::generate();

                let builder =
                    EventBuilder::new(Kind::ApplicationSpecificData, enc_key.to_bech32().unwrap())
                        .tags(vec![
                            Tag::identifier(ACCOUNT_D),
                            Tag::public_key(public_key),
                        ]);

                if let Ok(event) = builder.sign(&keys).await {
                    if let Err(e) = client.database().save_event(&event).await {
                        log::error!("Failed to save event: {e}");
                    };
                }
            }
        })
        .detach();
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

    pub fn logging_in(&self) -> bool {
        self.auto_logging_in_progress
    }

    pub(crate) fn set_logging_in(&mut self, status: bool, cx: &mut Context<Self>) {
        self.auto_logging_in_progress = status;
        cx.notify();
    }
}
