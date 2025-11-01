use anyhow::Error;
use cargo_packager_updater::semver::Version;
use cargo_packager_updater::Update;
use gpui::{App, AppContext, Context, Entity, Global, Task, Window};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use states::{app_state, BOOTSTRAP_RELAYS};

const APP_PUBKEY: &str = "npub1y9jvl5vznq49eh9f2gj7679v4042kj80lp7p8fte3ql2cr7hty7qsyca8q";

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

#[derive(Debug)]
pub struct AutoUpdater {
    /// Current status of the auto updater
    pub status: AutoUpdateStatus,

    /// Current version of the application
    pub version: Version,

    /// Background tasks
    _tasks: SmallVec<[Task<()>; 2]>,
}

impl AutoUpdater {
    /// Retrieve the global auto updater instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAutoUpdater>().0.clone()
    }

    /// Set the global auto updater instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAutoUpdater(state));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let mut tasks = smallvec![];

        tasks.push(
            // Subscribe to get the new update event in the bootstrap relays
            Self::subscribe_to_updates(cx),
        );

        Self {
            version,
            status: AutoUpdateStatus::Idle,
            _tasks: tasks,
        }
    }

    fn subscribe_to_updates(cx: &App) -> Task<()> {
        cx.background_spawn(async move {
            let client = app_state().client();
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
            let app_pubkey = PublicKey::parse(APP_PUBKEY).unwrap();

            let filter = Filter::new()
                .kind(Kind::ReleaseArtifactSet)
                .author(app_pubkey)
                .limit(1);

            if let Err(e) = client
                .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                .await
            {
                log::error!("Failed to subscribe to updates: {e}");
            };
        })
    }

    fn check_for_updates(cx: &App) -> Task<Result<Option<Update>, Error>> {
        cx.background_spawn(async move {
            let client = app_state().client();
            let app_pubkey = PublicKey::parse(APP_PUBKEY).unwrap();

            let filter = Filter::new()
                .kind(Kind::ReleaseArtifactSet)
                .author(app_pubkey)
                .limit(1);

            Ok(None)
        })
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
