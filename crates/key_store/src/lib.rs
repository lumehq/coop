use std::sync::{Arc, LazyLock};

use gpui::{App, AppContext, Context, Entity, Global, Task};
use smallvec::{smallvec, SmallVec};

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
}
