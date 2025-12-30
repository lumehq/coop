use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Error};
use gpui::{App, AppContext, Context, Entity, Global, SharedString, Subscription, Task};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use state::client;

const SETTINGS_IDENTIFIER: &str = "coop:settings";

pub fn init(cx: &mut App) {
    AppSettings::set_global(cx.new(AppSettings::new), cx)
}

/// Authentication mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum AuthenticationMode {
    Auto,
    #[default]
    Manual,
}

impl AuthenticationMode {
    pub fn is_auto(&self) -> bool {
        matches!(self, AuthenticationMode::Auto)
    }
}

/// Screening mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ScreeningMode {
    Disabled,
    Everyone,
    #[default]
    UnknownOnly,
}

impl ScreeningMode {
    pub fn is_unknown(&self) -> bool {
        matches!(self, ScreeningMode::UnknownOnly)
    }

    pub fn is_everyone(&self) -> bool {
        matches!(self, ScreeningMode::Everyone)
    }
}

/// Signer kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum SignerKind {
    Encryption,
    User,
    #[default]
    Auto,
}

/// Chat room configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    backup: bool,
    preferred_signer: SignerKind,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            backup: true,
            preferred_signer: SignerKind::default(),
        }
    }
}

/// Settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Hide user avatars.
    pub hide_avatar: bool,

    /// Automatically login on startup.
    pub autologin: bool,

    /// Screening mode.
    pub screening: ScreeningMode,

    /// Authentication mode.
    pub authentication: AuthenticationMode,

    /// User's preferred theme.
    pub preferred_theme: SharedString,

    /// List of trusted relays. Allow automatically authenticate to these relays.
    pub trusted_relays: HashSet<RelayUrl>,

    /// Default server for file uploads.
    pub file_server: Url,

    /// Chat room configuration.
    pub room_configs: HashMap<u64, RoomConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hide_avatar: false,
            autologin: false,
            screening: ScreeningMode::default(),
            authentication: AuthenticationMode::default(),
            preferred_theme: SharedString::new("Coop Default Theme"),
            trusted_relays: HashSet::default(),
            file_server: Url::parse("https://nostrmedia.com").unwrap(),
            room_configs: HashMap::default(),
        }
    }
}

impl Settings {
    pub fn is_trusted_relay(&self, url: &RelayUrl) -> bool {
        self.trusted_relays.contains(url)
    }
}

impl AsRef<Settings> for Settings {
    fn as_ref(&self) -> &Settings {
        self
    }
}

struct GlobalAppSettings(Entity<AppSettings>);

impl Global for GlobalAppSettings {}

/// Settings
pub struct AppSettings {
    pub settings: Settings,

    // Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    // Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl AppSettings {
    /// Retrieve the global settings instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAppSettings>().0.clone()
    }

    /// Set the global settings instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAppSettings(state));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        // Load settings from the database without the current user.
        // This will load the latest settings stored in the database.
        // These settings may not belong to the current user if the user has multiple accounts.
        let load_settings = Self::load_from_database(false, cx);

        let mut tasks = smallvec![];
        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Observe and automatically save settings on changes
            cx.observe_self(|this, cx| {
                this.save(cx);
            }),
        );

        tasks.push(
            // Load the initial settings
            cx.spawn(async move |this, cx| {
                if let Ok(settings) = load_settings.await {
                    this.update(cx, |this, cx| {
                        this.set_settings(settings, cx);
                    })
                    .ok();
                }
            }),
        );

        Self {
            settings: Settings::default(),
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    // Retrieve the settings
    pub fn settings(cx: &App) -> &Settings {
        cx.global::<GlobalAppSettings>()
            .0
            .clone()
            .read(cx)
            .settings
            .as_ref()
    }

    /// Load settings for current user from the database
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let task = Self::load_from_database(true, cx);

        self._tasks.push(cx.spawn(async move |this, cx| {
            if let Ok(settings) = task.await {
                this.update(cx, |this, cx| {
                    this.set_settings(settings, cx);
                })
                .ok();
            }
        }));
    }

    /// Load settings from the database
    fn load_from_database(current_user: bool, cx: &App) -> Task<Result<Settings, Error>> {
        let client = client();

        let mut filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(SETTINGS_IDENTIFIER)
            .limit(1);

        cx.background_spawn(async move {
            if current_user {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                // Add author fitler
                filter = filter.author(public_key);
            }

            if let Some(event) = client.database().query(filter).await?.first() {
                Ok(serde_json::from_str(&event.content).unwrap_or(Settings::default()))
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    /// Save settings for current user to the database
    fn save(&mut self, cx: &mut Context<Self>) {
        let Ok(content) = serde_json::to_string(self.settings.as_ref()) else {
            log::error!("Failed to serialize settings");
            return;
        };

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let ident = Tag::identifier(SETTINGS_IDENTIFIER);
            let keys = Keys::generate();

            // Construct the event
            let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
                .tag(ident)
                .build(public_key)
                .sign(&keys)
                .await?;

            // Save the event to the database
            client.database().save_event(&event).await?;

            Ok(())
        });

        // Run the task in the background, ignoring errors
        task.detach();
    }

    /// Update settings and notify the UI
    fn set_settings(&mut self, new_settings: Settings, cx: &mut Context<Self>) {
        self.settings = new_settings;
        cx.notify();
    }
}
