use anyhow::Error;
use cargo_packager_updater::semver::Version;
use cargo_packager_updater::{check_update, Config, Update};
use gpui::http_client::Url;
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task, Window};
use smallvec::{smallvec, SmallVec};
use states::constants::{APP_PUBKEY, APP_UPDATER_ENDPOINT};

pub fn init(cx: &mut App) {
    AutoUpdater::set_global(cx.new(AutoUpdater::new), cx);
}

struct GlobalAutoUpdater(Entity<AutoUpdater>);

impl Global for GlobalAutoUpdater {}

#[derive(Debug, Clone)]
pub enum AutoUpdateStatus {
    Idle,
    Checking,
    Checked { update: Box<Update> },
    Installing,
    Updated,
    Errored { msg: Box<String> },
}

impl AutoUpdateStatus {
    pub fn is_updating(&self) -> bool {
        matches!(self, Self::Checked { .. } | Self::Installing)
    }

    pub fn is_updated(&self) -> bool {
        matches!(self, Self::Updated)
    }

    pub fn checked(update: Update) -> Self {
        Self::Checked {
            update: Box::new(update),
        }
    }

    pub fn error(e: String) -> Self {
        Self::Errored { msg: Box::new(e) }
    }
}

pub struct AutoUpdater {
    pub status: AutoUpdateStatus,
    config: Config,
    version: Version,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl AutoUpdater {
    /// Retrieve the Global Auto Updater instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAutoUpdater>().0.clone()
    }

    /// Retrieve the Auto Updater instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalAutoUpdater>().0.read(cx)
    }

    /// Set the Global Auto Updater instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAutoUpdater(state));
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let config = cargo_packager_updater::Config {
            endpoints: vec![Url::parse(APP_UPDATER_ENDPOINT).expect("Endpoint is not valid")],
            pubkey: String::from(APP_PUBKEY),
            ..Default::default()
        };
        let version = Version::parse(env!("CARGO_PKG_VERSION")).expect("Failed to parse version");
        let mut subscriptions = smallvec![];

        subscriptions.push(cx.observe_new::<Self>(|this, window, cx| {
            if let Some(window) = window {
                this.check_for_updates(window, cx);
            }
        }));

        Self {
            status: AutoUpdateStatus::Idle,
            version,
            config,
            subscriptions,
        }
    }

    pub fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let current_version = self.version.clone();

        log::info!("Checking for updates...");
        self.set_status(AutoUpdateStatus::Checking, cx);

        let checking: Task<Result<Option<Update>, Error>> = cx.background_spawn(async move {
            if let Some(update) = check_update(current_version, config)? {
                Ok(Some(update))
            } else {
                Ok(None)
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Some(update)) = checking.await {
                this.update_in(cx, |this, window, cx| {
                    this.set_status(AutoUpdateStatus::checked(update), cx);
                    this.install_update(window, cx);
                })
                .ok();
            } else {
                this.update(cx, |this, cx| {
                    this.set_status(AutoUpdateStatus::Idle, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_status(AutoUpdateStatus::Installing, cx);

        if let AutoUpdateStatus::Checked { update } = self.status.clone() {
            let install: Task<Result<(), Error>> =
                cx.background_spawn(async move { Ok(update.download_and_install()?) });

            cx.spawn_in(window, async move |this, cx| {
                match install.await {
                    Ok(_) => {
                        this.update(cx, |this, cx| {
                            this.set_status(AutoUpdateStatus::Updated, cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        this.update(cx, |this, cx| {
                            this.set_status(AutoUpdateStatus::error(e.to_string()), cx);
                        })
                        .ok();
                    }
                };
            })
            .detach();
        }
    }

    fn set_status(&mut self, status: AutoUpdateStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }
}
