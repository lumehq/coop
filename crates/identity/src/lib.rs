use std::time::Duration;

use anyhow::{anyhow, Error};
use client_keys::ClientKeys;
use common::handle_auth::CoopAuthUrlHandler;
use global::constants::{ACCOUNT_D, NIP17_RELAYS, NIP65_RELAYS, NOSTR_CONNECT_TIMEOUT};
use global::nostr_client;
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
    public_key: Option<PublicKey>,
    logging_in: bool,
    has_dm_relays: Option<bool>,
    need_backup: Option<Keys>,
    need_onboarding: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Identity {
    /// Retrieve the Global Identity instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalIdentity>().0.clone()
    }

    /// Retrieve the Identity instance
    pub fn read_global(cx: &App) -> &Self {
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
                let auto_login = AppSettings::get_auto_login(cx);
                let has_client_keys = state.read(cx).has_keys();

                // Skip auto login if the user hasn't enabled auto login
                if has_client_keys && auto_login {
                    this.set_logging_in(true, cx);
                    this.load(window, cx);
                } else {
                    this.set_public_key(None, window, cx);
                }
            }),
        );

        Self {
            public_key: None,
            need_backup: None,
            has_dm_relays: None,
            need_onboarding: false,
            logging_in: false,
            subscriptions,
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_D)
                .limit(1);

            if let Some(event) = nostr_client().database().query(filter).await?.first_owned() {
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
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_public_key(None, window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn unload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_D);

            // Unset signer
            client.unset_signer().await;

            // Delete account
            client.database().delete(filter).await?;

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(_) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_public_key(None, window, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            };
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
                self.set_public_key(None, window, cx);
            }
        } else if let Ok(enc) = EncryptedSecretKey::from_bech32(secret) {
            self.login_with_keys(enc, window, cx);
        } else {
            window.push_notification(Notification::error("Secret Key is invalid"), cx);
            self.set_public_key(None, window, cx);
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
            self.set_public_key(None, window, cx);
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
                        window.push_notification(Notification::error(e.to_string()), cx);
                        this.update(cx, |this, cx| {
                            this.set_public_key(None, window, cx);
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
                .on_cancel(move |_, window, cx| {
                    entity
                        .update(cx, |this, cx| {
                            this.set_public_key(None, window, cx);
                        })
                        .ok();
                    // true to close the modal
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
                    // false to keep the modal open
                    false
                })
                .child(
                    div()
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
        let task: Task<Result<SecretKey, Error>> = cx.background_spawn(async move {
            let secret = enc.decrypt(&password)?;
            Ok(secret)
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(secret) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            // Update user's signer with decrypted secret key
                            this.set_signer(Keys::new(secret), window, cx);
                            // Close the current modal
                            window.close_modal(cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    error
                        .update(cx, |this, cx| {
                            *this = Some(e.to_string().into());
                            cx.notify();
                        })
                        .ok();
                }
            }
        })
        .detach();
    }

    /// Sets a new signer for the client and updates user identity
    pub fn set_signer<S>(&mut self, signer: S, window: &mut Window, cx: &mut Context<Self>)
    where
        S: NostrSigner + 'static,
    {
        let task: Task<Result<PublicKey, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let public_key = signer.get_public_key().await?;

            // Update signer
            client.set_signer(signer).await;

            // Subscribe for user metadata
            get_nip65_relays(public_key).await?;

            Ok(public_key)
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(public_key) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_public_key(Some(public_key), window, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    /// Creates a new identity with the given metadata
    pub fn new_identity(
        &mut self,
        metadata: Metadata,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keys = Keys::generate();
        let async_keys = keys.clone();

        let task: Task<Result<PublicKey, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let public_key = async_keys.public_key();

            // Update signer
            client.set_signer(async_keys).await;

            // Set metadata
            client.set_metadata(&metadata).await?;

            // Create relay list
            let relay_list = EventBuilder::new(Kind::RelayList, "").tags(
                NIP65_RELAYS
                    .into_iter()
                    .filter_map(|url| RelayUrl::parse(url).ok())
                    .map(|url| Tag::relay_metadata(url, None)),
            );

            // Create messaging relay list
            let dm_relay = EventBuilder::new(Kind::InboxRelays, "").tags(
                NIP17_RELAYS
                    .into_iter()
                    .filter_map(|url| RelayUrl::parse(url).ok())
                    .map(Tag::relay),
            );

            // Set user's NIP65 relays
            client.send_event_builder(relay_list).await?;

            // Set user's NIP17 relays
            client.send_event_builder(dm_relay).await?;

            // Get user's NIP65 relays
            get_nip65_relays(public_key).await?;

            Ok(public_key)
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(public_key) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_public_key(Some(public_key), window, cx);
                            this.set_need_backup(Some(keys), cx);
                            this.set_need_onboarding(cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    /// Clear the user's need backup status
    pub fn clear_need_backup(&mut self, password: String, cx: &mut Context<Self>) {
        if let Some(keys) = self.need_backup.as_ref() {
            // Encrypt the keys then writing them to keychain
            self.write_keys(keys, password, cx);
            // Clear the needed backup keys
            self.need_backup = None;
            cx.notify();
        }
    }

    /// Set the user's need backup status
    pub(crate) fn set_need_backup(&mut self, keys: Option<Keys>, cx: &mut Context<Self>) {
        self.need_backup = keys;
        cx.notify();
    }

    /// Set the user's need onboarding status
    pub(crate) fn set_need_onboarding(&mut self, cx: &mut Context<Self>) {
        self.need_onboarding = true;
        cx.notify();
    }

    /// Returns true if the user needs backup their keys
    pub fn need_backup(&self) -> Option<&Keys> {
        self.need_backup.as_ref()
    }

    /// Returns true if the user needs onboarding
    pub fn need_onboarding(&self) -> bool {
        self.need_onboarding
    }

    /// Writes the bunker uri to the database
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
            let client = nostr_client();
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

    /// Writes the keys to the database
    pub fn write_keys(&self, keys: &Keys, password: String, cx: &mut Context<Self>) {
        let keys = keys.to_owned();
        let public_key = keys.public_key();

        cx.background_spawn(async move {
            if let Ok(enc_key) =
                EncryptedSecretKey::new(keys.secret_key(), &password, 8, KeySecurity::Unknown)
            {
                let client = nostr_client();
                let content = enc_key.to_bech32().unwrap();

                let builder = EventBuilder::new(Kind::ApplicationSpecificData, content).tags(vec![
                    Tag::identifier(ACCOUNT_D),
                    Tag::public_key(public_key),
                ]);

                if let Ok(event) = builder.sign(&Keys::generate()).await {
                    if let Err(e) = client.database().save_event(&event).await {
                        log::error!("Failed to save event: {e}");
                    };
                }
            }
        })
        .detach();
    }

    /// Sets the public key of the identity
    pub(crate) fn set_public_key(
        &mut self,
        public_key: Option<PublicKey>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.public_key = public_key;
        cx.notify();
    }

    /// Returns the current identity's public key
    pub fn public_key(&self) -> Option<PublicKey> {
        self.public_key
    }

    /// Returns true if a signer is currently set
    pub fn has_signer(&self) -> bool {
        self.public_key.is_some()
    }

    /// Returns true if the identity has DM Relays
    pub fn has_dm_relays(&self) -> Option<bool> {
        self.has_dm_relays
    }

    /// Returns true if the identity is currently logging in
    pub fn logging_in(&self) -> bool {
        self.logging_in
    }

    /// Sets the DM Relays status of the identity
    pub fn set_has_dm_relays(&mut self, cx: &mut Context<Self>) {
        self.has_dm_relays = Some(true);
        cx.notify();
    }

    /// Sets the logging in status of the identity
    pub(crate) fn set_logging_in(&mut self, status: bool, cx: &mut Context<Self>) {
        self.logging_in = status;
        cx.notify();
    }
}

async fn get_nip65_relays(public_key: PublicKey) -> Result<(), Error> {
    let client = nostr_client();
    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
    let sub_id = SubscriptionId::new("nip65-relays");
    let filter = Filter::new()
        .kind(Kind::RelayList)
        .author(public_key)
        .limit(1);

    client.subscribe_with_id(sub_id, filter, Some(opts)).await?;

    Ok(())
}
