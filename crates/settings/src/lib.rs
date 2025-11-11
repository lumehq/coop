use anyhow::{anyhow, Error};
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;

const SETTINGS_IDENTIFIER: &str = "coop:settings";

pub fn init(cx: &mut App) {
    AppSettings::set_global(cx.new(AppSettings::new), cx)
}

macro_rules! setting_accessors {
    ($(pub $field:ident: $type:ty),* $(,)?) => {
        impl AppSettings {
            $(
                paste::paste! {
                    pub fn [<get_ $field>](cx: &App) -> $type {
                        Self::global(cx).read(cx).setting_values.$field.clone()
                    }

                    pub fn [<update_ $field>](value: $type, cx: &mut App) {
                        Self::global(cx).update(cx, |this, cx| {
                            this.setting_values.$field = value;
                            cx.notify();
                        });
                    }
                }
            )*
        }
    };
}

setting_accessors! {
    pub media_server: Url,
    pub proxy_user_avatars: bool,
    pub hide_user_avatars: bool,
    pub backup_messages: bool,
    pub screening: bool,
    pub contact_bypass: bool,
    pub auto_login: bool,
    pub auto_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub media_server: Url,
    pub proxy_user_avatars: bool,
    pub hide_user_avatars: bool,
    pub backup_messages: bool,
    pub screening: bool,
    pub contact_bypass: bool,
    pub auto_login: bool,
    pub auto_auth: bool,
    pub authenticated_relays: Vec<RelayUrl>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            media_server: Url::parse("https://nostrmedia.com").unwrap(),
            proxy_user_avatars: true,
            hide_user_avatars: false,
            backup_messages: true,
            screening: true,
            contact_bypass: true,
            auto_login: false,
            auto_auth: true,
            authenticated_relays: vec![],
        }
    }
}

impl AsRef<Settings> for Settings {
    fn as_ref(&self) -> &Settings {
        self
    }
}

struct GlobalAppSettings(Entity<AppSettings>);

impl Global for GlobalAppSettings {}

pub struct AppSettings {
    setting_values: Settings,

    // Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    // Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl AppSettings {
    /// Retrieve the Global Settings instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAppSettings>().0.clone()
    }

    /// Set the Global Settings instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAppSettings(state));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let load_settings = Self::_load_settings(false, cx);

        let mut tasks = smallvec![];
        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Observe and automatically save settings on changes
            cx.observe_self(|this, cx| {
                this.set_settings(cx);
            }),
        );

        tasks.push(
            // Load the initial settings
            cx.spawn(async move |this, cx| {
                if let Ok(settings) = load_settings.await {
                    this.update(cx, |this, cx| {
                        this.setting_values = settings;
                        cx.notify();
                    })
                    .ok();
                }
            }),
        );

        Self {
            setting_values: Settings::default(),
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    pub fn load_settings(&mut self, cx: &mut Context<Self>) {
        let task = Self::_load_settings(true, cx);

        self._tasks.push(
            // Run task in the background
            cx.spawn(async move |this, cx| {
                if let Ok(settings) = task.await {
                    this.update(cx, |this, cx| {
                        this.setting_values = settings;
                        cx.notify();
                    })
                    .ok();
                }
            }),
        );
    }

    fn _load_settings(user: bool, cx: &App) -> Task<Result<Settings, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        cx.background_spawn(async move {
            let mut filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(SETTINGS_IDENTIFIER)
                .limit(1);

            if user {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                filter = filter.author(public_key);
            }

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                Ok(serde_json::from_str(&event.content).unwrap_or(Settings::default()))
            } else {
                Err(anyhow!("Not found"))
            }
        })
    }

    pub fn set_settings(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        if let Ok(content) = serde_json::to_string(&self.setting_values) {
            let task: Task<Result<(), Error>> = cx.background_spawn(async move {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;

                let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
                    .tag(Tag::identifier(SETTINGS_IDENTIFIER))
                    .build(public_key)
                    .sign(&Keys::generate())
                    .await?;

                client.database().save_event(&event).await?;

                Ok(())
            });

            task.detach();
        }
    }

    /// Check if auto authentication is enabled
    pub fn is_auto_auth(&self) -> bool {
        !self.setting_values.authenticated_relays.is_empty() && self.setting_values.auto_auth
    }

    /// Check if a relay is authenticated
    pub fn is_authenticated(&self, url: &RelayUrl) -> bool {
        self.setting_values.authenticated_relays.contains(url)
    }

    /// Push a relay to the authenticated relays list
    pub fn push_relay(&mut self, relay_url: &RelayUrl, cx: &mut Context<Self>) {
        if !self.is_authenticated(relay_url) {
            self.setting_values
                .authenticated_relays
                .push(relay_url.to_owned());
            cx.notify();
        }
    }
}
