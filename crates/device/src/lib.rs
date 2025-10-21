use std::sync::{Arc, LazyLock};

use anyhow::{anyhow, Error};
use gpui::{
    App, AppContext, AsyncWindowContext, Context, Entity, Global, ParentElement, SharedString,
    Styled, Task, WeakEntity, Window,
};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use states::app_state;
use states::constants::APP_NAME;
use states::state::{Announcement, SignalKind};
use theme::ActiveTheme;
use ui::modal::ModalButtonProps;
use ui::{h_flex, v_flex, ContextModal};

use crate::keystore::{FileProvider, KeyItem, KeyStore, KeyringProvider};

pub mod keystore;

static DISABLE_KEYRING: LazyLock<bool> =
    LazyLock::new(|| std::env::var("DISABLE_KEYRING").is_ok_and(|value| !value.is_empty()));

pub fn init(window: &mut Window, cx: &mut App) {
    Device::set_global(cx.new(|cx| Device::new(window, cx)), cx);
}

struct GlobalDevice(Entity<Device>);

impl Global for GlobalDevice {}

pub struct Device {
    /// Key Store for storing credentials
    pub keystore: Arc<dyn KeyStore>,

    /// Whether the keystore has been initialized
    pub initialized: bool,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Client keys entity, used for communication between devices
    pub client_keys: Entity<Option<Arc<dyn NostrSigner>>>,

    /// NIP-4e: https://github.com/nostr-protocol/nips/blob/per-device-keys/4e.md
    ///
    /// Encryption keys entity, used for encryption and decryption
    pub encryption_keys: Entity<Option<Arc<dyn NostrSigner>>>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 2]>,
}

impl Device {
    /// Retrieve the global keys state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalDevice>().0.clone()
    }

    /// Set the global keys instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalDevice(state));
    }

    /// Create a new keys instance
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client_keys = cx.new(|_| None);
        let encryption_keys = cx.new(|_| None);

        // Use the file system for keystore in development or when the user specifies it
        let use_file_keystore = cfg!(debug_assertions) || *DISABLE_KEYRING;

        // Construct the keystore
        let keystore: Arc<dyn KeyStore> = if use_file_keystore {
            Arc::new(FileProvider::default())
        } else {
            Arc::new(KeyringProvider)
        };

        // Only used for testing keyring availability on the user's system
        let read_credential = cx.read_credentials("Coop");

        let mut tasks = smallvec![];

        tasks.push(
            // Verify the keyring availability
            cx.spawn(async move |this, cx| {
                let result = read_credential.await;

                this.update(cx, |this, cx| {
                    if let Err(e) = result {
                        log::error!("Keyring error: {e}");
                        // For Linux:
                        // The user has not installed secret service on their system
                        // Fall back to the file provider
                        this.keystore = Arc::new(FileProvider::default());
                    }
                    this.initialized = true;
                    cx.notify();
                })
                .ok();
            }),
        );

        tasks.push(
            // Continuously handle signals from the application state
            cx.spawn_in(window, async move |this, cx| {
                Self::handle_signals(this, cx).await
            }),
        );

        Self {
            client_keys,
            encryption_keys,
            keystore,
            initialized: false,
            _tasks: tasks,
        }
    }

    /// Handle signals from the application state
    async fn handle_signals(view: WeakEntity<Device>, cx: &mut AsyncWindowContext) {
        let states = app_state();

        while let Ok(signal) = states.signal().receiver().recv_async().await {
            view.update_in(cx, |this, window, cx| {
                match signal {
                    SignalKind::EncryptionNotSet => {
                        this.new_encryption(window, cx);
                    }
                    SignalKind::EncryptionSet(announcement) => {
                        this.load_encryption(announcement, window, cx);
                    }
                    _ => {}
                };
            })
            .ok();
        }
    }

    /// Returns the keystore.
    pub fn keystore(&self) -> Arc<dyn KeyStore> {
        Arc::clone(&self.keystore)
    }

    /// Returns true if the keystore is a file keystore.
    pub fn is_using_file_keystore(&self) -> bool {
        self.keystore.name() == "file"
    }

    /// Encrypt and store a key in the local database.
    pub fn set_local_key(&self, kind: KeyItem, value: String, cx: &App) -> Task<Result<(), Error>> {
        cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Encrypt the value
            let content = signer.nip44_encrypt(&public_key, value.as_ref()).await?;

            // Construct the application data event
            let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
                .tag(Tag::identifier(kind))
                .build(public_key)
                .sign(&Keys::generate())
                .await?;

            // Save the event to the database
            client.database().save_event(&event).await?;

            Ok(())
        })
    }

    /// Get and decrypt a key from the local database.
    pub fn load_local_key(&self, kind: KeyItem, cx: &App) -> Task<Result<Keys, Error>> {
        cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(kind)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first() {
                let content = signer.nip44_decrypt(&public_key, &event.content).await?;
                let secret = SecretKey::parse(&content)?;
                let keys = Keys::new(secret);

                Ok(keys)
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    /// Set the client keys
    pub fn set_client_keys(&mut self, keys: Keys, cx: &mut Context<Self>) {
        self.client_keys.update(cx, |this, cx| {
            *this = Some(Arc::new(keys));
            cx.notify();
        });
    }

    /// Set the encryption keys
    pub fn set_encryption_keys(&mut self, keys: Keys, cx: &mut Context<Self>) {
        self.encryption_keys.update(cx, |this, cx| {
            *this = Some(Arc::new(keys));
            cx.notify();
        });
    }

    /// Load the dedicated keys for the current device (client)
    pub fn load_client_keys(&mut self, cx: &mut Context<Self>) {
        let task = self.load_local_key(KeyItem::Client, cx);

        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| {
                match result {
                    Ok(keys) => {
                        this.set_client_keys(keys, cx);
                    }
                    Err(_) => {
                        this.set_client_keys(Keys::generate(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn new_encryption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = Keys::generate();
        let secret = keys.secret_key().to_secret_hex();
        let task = self.set_local_key(KeyItem::Encryption, secret, cx);

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(_) => {
                        this.set_encryption_keys(keys, cx);
                    }
                    Err(e) => {
                        // TODO: handle error
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn load_encryption(&mut self, ann: Announcement, window: &mut Window, cx: &mut Context<Self>) {
        let task = self.load_local_key(KeyItem::Encryption, cx);

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(keys) => {
                        if ann.public_key() == keys.public_key() {
                            this.set_encryption_keys(keys, cx);
                        } else {
                            this.request_encryption(ann, window, cx);
                        }
                    }
                    Err(_) => {
                        this.request_encryption(ann, window, cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn request_encryption(
        &mut self,
        ann: Announcement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Client Keys must be known at this point
        let Some(client_keys) = self.client_keys.read(cx).clone() else {
            window.push_notification("Client Keys is required", cx);
            return;
        };

        let task: Task<Result<Option<Keys>, Error>> = cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let client_pubkey = client_keys.get_public_key().await?;

            let filter = Filter::new()
                .kind(Kind::Custom(4455))
                .author(public_key)
                .pubkey(client_pubkey)
                .limit(1);

            match client.database().query(filter).await?.first_owned() {
                Some(event) => {
                    // Found encryption keys shared by other devices
                    if let Some(root_device) = event
                        .tags
                        .find(TagKind::custom("P"))
                        .and_then(|tag| tag.content())
                        .and_then(|content| PublicKey::parse(content).ok())
                        .as_ref()
                    {
                        let payload = event.content.as_str();
                        let decrypted = client_keys.nip44_decrypt(root_device, payload).await?;

                        let secret = SecretKey::from_hex(&decrypted)?;
                        let keys = Keys::new(secret);

                        return Ok(Some(keys));
                    } else {
                        return Err(anyhow!("Invalid event"));
                    }
                }
                None => {
                    // Construct encryption keys request event
                    let event = EventBuilder::new(Kind::Custom(4454), "")
                        .tags(vec![Tag::client(APP_NAME), Tag::public_key(client_pubkey)])
                        .sign(&signer)
                        .await?;

                    // Send a request for encryption keys from other devices
                    client.send_event(&event).await?;
                }
            }

            Ok(None)
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Some(keys)) => {
                        this.set_encryption_keys(keys, cx);
                    }
                    Ok(None) => {
                        this.render_wait_for_approval(ann, window, cx);
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn render_wait_for_approval(&mut self, ann: Announcement, window: &mut Window, cx: &mut App) {
        let client_name = SharedString::from(ann.client().to_string());
        let Ok(public_key) = ann.public_key().to_bech32();

        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .alert()
                .button_props(ModalButtonProps::default().ok_text("Hide"))
                .title("Wait for Approval")
                .child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .child("Please open the other client and approve the request.")
                        .child("Encryption keys is stored in:")
                        .child(
                            v_flex()
                                .justify_center()
                                .items_center()
                                .h_16()
                                .w_full()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().elevated_surface_background)
                                .child(client_name.clone()),
                        )
                        .child(
                            h_flex()
                                .h_7()
                                .w_full()
                                .px_1p5()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().elevated_surface_background)
                                .child(SharedString::from(&public_key)),
                        ),
                )
        });
    }
}
