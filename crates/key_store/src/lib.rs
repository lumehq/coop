use std::sync::{Arc, LazyLock};

use gpui::{App, AppContext, Context, Entity, Global, Task};
use smallvec::{SmallVec, smallvec};

use crate::backend::{FileProvider, KeyBackend, KeyringProvider};

pub mod backend;

static DISABLE_KEYRING: LazyLock<bool> =
    LazyLock::new(|| std::env::var("DISABLE_KEYRING").is_ok_and(|value| !value.is_empty()));

pub fn init(cx: &mut App) {
    KeyStore::set_global(cx.new(KeyStore::new), cx);
}

struct GlobalKeyStore(Entity<KeyStore>);

impl Global for GlobalKeyStore {}

pub struct KeyStore {
    /// Key Store for storing credentials
    pub backend: Arc<dyn KeyBackend>,

    /// Whether the keystore has been initialized
    pub initialized: bool,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl KeyStore {
    /// Retrieve the global keys state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalKeyStore>().0.clone()
    }

    /// Set the global keys instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalKeyStore(state));
    }

    /// Create a new keys instance
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        // Use the file system for keystore in development or when the user specifies it
        let use_file_keystore = cfg!(debug_assertions) || *DISABLE_KEYRING;

        // Construct the key backend
        let backend: Arc<dyn KeyBackend> = if use_file_keystore {
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
                        this.backend = Arc::new(FileProvider::default());
                    }
                    this.initialized = true;
                    cx.notify();
                })
                .ok();
            }),
        );

        Self {
            backend,
            initialized: false,
            _tasks: tasks,
        }
    }

    /// Returns the key backend.
    pub fn backend(&self) -> Arc<dyn KeyBackend> {
        Arc::clone(&self.backend)
    }

    /// Returns true if the keystore is a file key backend.
    pub fn is_using_file_keystore(&self) -> bool {
        self.backend.name() == "file"
    }

    /*
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
                        .tags(vec![
                            Tag::client(APP_NAME),
                            Tag::custom(TagKind::custom("pubkey"), vec![client_pubkey]),
                        ])
                        .sign(&signer)
                        .await?;

                    // Send a request for encryption keys from other devices
                    client.send_event(&event).await?;

                    // Create a unique ID to control the subscription later
                    let subscription_id = SubscriptionId::new("request");

                    let filter = Filter::new()
                        .kind(Kind::Custom(4455))
                        .author(public_key)
                        .pubkey(client_pubkey)
                        .since(Timestamp::now());

                    // Subscribe to the approval response event
                    client
                        .subscribe_with_id(subscription_id, filter, None)
                        .await?;
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

    fn response_encryption(
        &mut self,
        target: PublicKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Client Keys must be known at this point
        let Some(client_keys) = self.client_keys.read(cx).clone() else {
            window.push_notification("Client Keys is required", cx);
            return;
        };

        // Get the encryption keys
        let get_encryption = self.load_local_key(KeyItem::Encryption, cx);

        // Send a response to the request
        let response: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = app_state().client();
            let client_pubkey = client_keys.get_public_key().await?;
            let encryption = get_encryption.await?;

            // Encrypt the encryption keys with the client's signer
            let payload = client_keys
                .nip44_encrypt(&target, &encryption.secret_key().to_secret_hex())
                .await?;

            // Construct the response event
            //
            // P tag: the current client's public key
            // p tag: the requester's public key
            let event = EventBuilder::new(Kind::Custom(4455), payload)
                .tags(vec![
                    Tag::custom(TagKind::custom("P"), vec![client_pubkey]),
                    Tag::public_key(target),
                ])
                .sign(&client_keys)
                .await?;

            // Get the current user's signer and public key
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            // Get the current user's relay list
            let urls: Vec<RelayUrl> = client
                .database()
                .relay_list(public_key)
                .await?
                .into_iter()
                .filter_map(|(url, metadata)| {
                    if metadata.is_none() || metadata == Some(RelayMetadata::Read) {
                        Some(url)
                    } else {
                        None
                    }
                })
                .collect();

            // Send the response event to the user's relay list
            client.send_event_to(urls, &event).await?;

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = response.await;

            this.update_in(cx, |_this, window, cx| {
                match result {
                    Ok(_) => {
                        window.clear_notifications(cx);
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

    fn receive_encryption(&mut self, res: Response, window: &mut Window, cx: &mut Context<Self>) {
        // Client Keys must be known at this point
        let Some(client_keys) = self.client_keys.read(cx).clone() else {
            window.push_notification("Client Keys is required", cx);
            return;
        };

        let task: Task<Result<Keys, Error>> = cx.background_spawn(async move {
            let public_key = res.public_key();
            let payload = res.payload();

            // Decrypt the payload using the client keys
            let decrypted = client_keys.nip44_decrypt(&public_key, payload).await?;

            // Construct the newly received encryption keys
            let secret = SecretKey::parse(&decrypted)?;
            let keys = Keys::new(secret);

            Ok(keys)
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(keys) => {
                        this.set_encryption_keys(keys, cx);
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
        let public_key = ann.public_key().to_bech32().unwrap();

        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .alert()
                .button_props(ModalButtonProps::default().ok_text(t!("common.hide")))
                .title(shared_t!("pending_encryption.label"))
                .child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .child(shared_t!("pending_encryption.body_1"))
                        .child(
                            v_flex()
                                .justify_center()
                                .items_center()
                                .h_16()
                                .w_full()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().elevated_surface_background)
                                .font_semibold()
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
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().text_muted)
                                .child(shared_t!("pending_encryption.body_2")),
                        ),
                )
        });
    }

    fn render_request(&mut self, ann: Announcement, window: &mut Window, cx: &mut Context<Self>) {
        let client_name = SharedString::from(ann.client().to_string());
        let target = ann.public_key();
        let view = cx.entity().downgrade();

        let note = Notification::new()
            .custom_id(SharedString::from(ann.id().to_hex()))
            .autohide(false)
            .icon(IconName::Info)
            .title(shared_t!("request_encryption.label"))
            .content(move |_window, cx| {
                v_flex()
                    .gap_2()
                    .text_sm()
                    .child(shared_t!("request_encryption.body"))
                    .child(
                        v_flex()
                            .py_1()
                            .px_1p5()
                            .rounded_sm()
                            .text_xs()
                            .bg(cx.theme().warning_background)
                            .text_color(cx.theme().warning_foreground)
                            .child(client_name.clone()),
                    )
                    .into_any_element()
            })
            .action(move |_window, _cx| {
                let view = view.clone();

                Button::new("approve")
                    .label(t!("common.approve"))
                    .small()
                    .primary()
                    .loading(false)
                    .disabled(false)
                    .on_click(move |_ev, window, cx| {
                        view.update(cx, |this, cx| {
                            this.response_encryption(target, window, cx);
                        })
                        .ok();
                    })
            });

        window.push_notification(note, cx);
    }
    */
}
