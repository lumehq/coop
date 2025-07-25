use anyhow::anyhow;
use global::constants::SETTINGS_D;
use global::nostr_client;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

pub fn init(cx: &mut App) {
    let state = cx.new(AppSettings::new);

    // Observe for state changes and save settings to database
    state.update(cx, |this, cx| {
        this.subscriptions
            .push(cx.observe(&state, |this, _state, cx| {
                this.set_settings(cx);
            }));
    });

    AppSettings::set_global(state, cx);
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
}

impl AsRef<Settings> for Settings {
    fn as_ref(&self) -> &Settings {
        self
    }
}

struct GlobalAppSettings(Entity<AppSettings>);

impl Global for GlobalAppSettings {}

pub struct AppSettings {
    pub settings: Settings,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl AppSettings {
    /// Retrieve the Global Settings instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAppSettings>().0.clone()
    }

    /// Retrieve the Settings instance
    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalAppSettings>().0.read(cx)
    }

    /// Set the Global Settings instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAppSettings(state));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let settings = Settings {
            media_server: Url::parse("https://nostrmedia.com").unwrap(),
            proxy_user_avatars: true,
            hide_user_avatars: false,
            backup_messages: true,
            screening: true,
            contact_bypass: true,
            auto_login: false,
        };

        let mut subscriptions = smallvec![];

        subscriptions.push(cx.observe_new::<Self>(|this, _window, cx| {
            this.get_settings_from_db(cx);
        }));

        Self {
            settings,
            subscriptions,
        }
    }

    pub(crate) fn get_settings_from_db(&self, cx: &mut Context<Self>) {
        let task: Task<Result<Settings, anyhow::Error>> = cx.background_spawn(async move {
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(SETTINGS_D)
                .limit(1);

            if let Some(event) = nostr_client().database().query(filter).await?.first_owned() {
                log::info!("Successfully loaded settings from database");
                Ok(serde_json::from_str(&event.content)?)
            } else {
                Err(anyhow!("Not found"))
            }
        });

        cx.spawn(async move |this, cx| {
            if let Ok(settings) = task.await {
                this.update(cx, |this, cx| {
                    this.settings = settings;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn set_settings(&self, cx: &mut Context<Self>) {
        if let Ok(content) = serde_json::to_string(&self.settings) {
            cx.background_spawn(async move {
                let keys = Keys::generate();

                if let Ok(event) = EventBuilder::new(Kind::ApplicationSpecificData, content)
                    .tags(vec![Tag::identifier(SETTINGS_D)])
                    .sign(&keys)
                    .await
                {
                    if let Err(e) = nostr_client().database().save_event(&event).await {
                        log::error!("Failed to save user settings: {e}");
                    } else {
                        log::info!("New settings have been saved successfully");
                    }
                }
            })
            .detach();
        }
    }
}
