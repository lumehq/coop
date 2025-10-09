use anyhow::anyhow;
use global::app_state;
use global::app_state::AppIdentifierTag;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

pub fn init(cx: &mut App) {
    let state = cx.new(AppSettings::new);

    // Observe for state changes and save settings to database
    state.update(cx, |this, cx| {
        this._subscriptions
            .push(cx.observe(&state, |this, _state, cx| {
                this.set_settings(cx);
            }));
    });

    AppSettings::set_global(state, cx);
}

macro_rules! setting_accessors {
    ($(pub $field:ident: $type:ty),* $(,)?) => {
        impl AppSettings {
            $(
                paste::paste! {
                    pub fn [<get_ $field>](cx: &App) -> $type {
                        Self::read_global(cx).setting_values.$field.clone()
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

#[derive(Serialize, Deserialize)]
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
    _subscriptions: SmallVec<[Subscription; 1]>,
}

impl AppSettings {
    /// Retrieve the Global Settings instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAppSettings>().0.clone()
    }

    /// Retrieve the Settings instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalAppSettings>().0.read(cx)
    }

    /// Set the Global Settings instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAppSettings(state));
    }

    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            setting_values: Settings::default(),
            _subscriptions: smallvec![],
        }
    }

    pub fn load_settings(&self, cx: &mut Context<Self>) {
        let task: Task<Result<Settings, anyhow::Error>> = cx.background_spawn(async move {
            let app_state = app_state();

            if let Ok(content) = app_state.load_from_db(AppIdentifierTag::Setting).await {
                Ok(serde_json::from_str(&content).unwrap_or(Settings::default()))
            } else {
                Err(anyhow!("Not found"))
            }
        });

        cx.spawn(async move |this, cx| {
            if let Ok(settings) = task.await {
                this.update(cx, |this, cx| {
                    this.setting_values = settings;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn set_settings(&self, cx: &mut Context<Self>) {
        if let Ok(content) = serde_json::to_string(&self.setting_values) {
            cx.background_spawn(async move {
                let app_state = app_state();

                app_state
                    .write_to_db(&content, AppIdentifierTag::Setting)
                    .await
                    .ok()
            })
            .detach();
        }
    }

    pub fn is_auto_auth(&self) -> bool {
        !self.setting_values.authenticated_relays.is_empty() && self.setting_values.auto_auth
    }

    pub fn is_authenticated(&self, url: &RelayUrl) -> bool {
        self.setting_values.authenticated_relays.contains(url)
    }

    pub fn push_relay(&mut self, relay_url: &RelayUrl, cx: &mut Context<Self>) {
        if !self.is_authenticated(relay_url) {
            self.setting_values
                .authenticated_relays
                .push(relay_url.to_owned());
            cx.notify();
        }
    }
}
