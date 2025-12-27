use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::BOOTSTRAP_RELAYS;
use gpui::http_client::{AsyncBody, HttpClient};
use gpui::{
    App, AppContext, AsyncApp, BackgroundExecutor, Context, Entity, Global, Subscription, Task,
};
use nostr_sdk::prelude::*;
use semver::Version;
use smallvec::{smallvec, SmallVec};
use smol::fs::File;
use smol::process::Command;
use state::client;

const APP_PUBKEY: &str = "npub1y9jvl5vznq49eh9f2gj7679v4042kj80lp7p8fte3ql2cr7hty7qsyca8q";

pub fn init(cx: &mut App) {
    AutoUpdater::set_global(cx.new(AutoUpdater::new), cx);
}

struct GlobalAutoUpdater(Entity<AutoUpdater>);

impl Global for GlobalAutoUpdater {}

#[cfg(not(target_os = "windows"))]
struct InstallerDir(tempfile::TempDir);

#[cfg(not(target_os = "windows"))]
impl InstallerDir {
    async fn new() -> Result<Self, Error> {
        Ok(Self(
            tempfile::Builder::new()
                .prefix("coop-auto-update")
                .tempdir()?,
        ))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

#[cfg(target_os = "windows")]
struct InstallerDir(PathBuf);

#[cfg(target_os = "windows")]
impl InstallerDir {
    async fn new() -> Result<Self, Error> {
        let installer_dir = std::env::current_exe()?
            .parent()
            .context("No parent dir for Coop.exe")?
            .join("updates");

        if smol::fs::metadata(&installer_dir).await.is_ok() {
            smol::fs::remove_dir_all(&installer_dir).await?;
        }

        smol::fs::create_dir(&installer_dir).await?;

        Ok(Self(installer_dir))
    }

    fn path(&self) -> &Path {
        self.0.as_path()
    }
}

struct MacOsUnmounter<'a> {
    mount_path: PathBuf,
    background_executor: &'a BackgroundExecutor,
}

impl Drop for MacOsUnmounter<'_> {
    fn drop(&mut self) {
        let mount_path = std::mem::take(&mut self.mount_path);

        self.background_executor
            .spawn(async move {
                let unmount_output = Command::new("hdiutil")
                    .args(["detach", "-force"])
                    .arg(&mount_path)
                    .output()
                    .await;

                match unmount_output {
                    Ok(output) if output.status.success() => {
                        log::info!("Successfully unmounted the disk image");
                    }
                    Ok(output) => {
                        log::error!(
                            "Failed to unmount disk image: {:?}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    Err(error) => {
                        log::error!("Error while trying to unmount disk image: {:?}", error);
                    }
                }
            })
            .detach();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutoUpdateStatus {
    Idle,
    Checking,
    Checked { files: Vec<EventId> },
    Installing,
    Updated,
    Errored { msg: Box<String> },
}

impl AsRef<AutoUpdateStatus> for AutoUpdateStatus {
    fn as_ref(&self) -> &AutoUpdateStatus {
        self
    }
}

impl AutoUpdateStatus {
    pub fn is_updating(&self) -> bool {
        matches!(self, Self::Checked { .. } | Self::Installing)
    }

    pub fn is_updated(&self) -> bool {
        matches!(self, Self::Updated)
    }

    pub fn checked(files: Vec<EventId>) -> Self {
        Self::Checked { files }
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

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Background tasks
    _tasks: SmallVec<[Task<Result<(), Error>>; 2]>,
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
        let async_version = version.clone();

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        tasks.push(
            // Subscribe to get the new update event in the bootstrap relays
            cx.background_spawn(async move { Self::subscribe_to_updates().await }),
        );

        tasks.push(
            // Check for updates
            cx.spawn(async move |this, cx| {
                // Check for updates after 2 minutes
                cx.background_executor()
                    .timer(Duration::from_secs(120))
                    .await;

                // Update the status to checking
                this.update(cx, |this, cx| {
                    this.set_status(AutoUpdateStatus::Checking, cx);
                })
                .ok();

                let result = cx
                    .background_spawn(async move { Self::check_for_updates(async_version).await })
                    .await;

                match result {
                    Ok(ids) => {
                        // Update the status to downloading
                        _ = this.update(cx, |this, cx| {
                            this.set_status(AutoUpdateStatus::checked(ids), cx);
                        });
                    }
                    Err(e) => {
                        _ = this.update(cx, |this, cx| {
                            this.set_status(AutoUpdateStatus::Idle, cx);
                        });
                        log::warn!("{e}");
                    }
                }

                Ok(())
            }),
        );

        subscriptions.push(
            // Observe the status
            cx.observe_self(|this, cx| {
                if let AutoUpdateStatus::Checked { files } = this.status.clone() {
                    this.get_latest_release(&files, cx);
                }
            }),
        );

        Self {
            status: AutoUpdateStatus::Idle,
            version,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn subscribe_to_updates() -> Result<(), Error> {
        let client = client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let app_pubkey = PublicKey::parse(APP_PUBKEY)?;

        let filter = Filter::new()
            .kind(Kind::ReleaseArtifactSet)
            .author(app_pubkey)
            .limit(1);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    async fn check_for_updates(version: Version) -> Result<Vec<EventId>, Error> {
        let client = client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let app_pubkey = PublicKey::parse(APP_PUBKEY).unwrap();

        let filter = Filter::new()
            .kind(Kind::ReleaseArtifactSet)
            .author(app_pubkey)
            .limit(1);

        if let Some(event) = client.database().query(filter).await?.first_owned() {
            let new_version: Version = event
                .tags
                .find(TagKind::d())
                .and_then(|tag| tag.content())
                .and_then(|content| content.split("@").last())
                .and_then(|content| Version::parse(content).ok())
                .context("Failed to parse version")?;

            if new_version > version {
                // Get all file metadata event ids
                let ids: Vec<EventId> = event.tags.event_ids().copied().collect();

                let filter = Filter::new()
                    .kind(Kind::FileMetadata)
                    .author(app_pubkey)
                    .ids(ids.clone());

                // Get all files for this release
                client
                    .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                    .await?;

                Ok(ids)
            } else {
                Err(anyhow!("No update available"))
            }
        } else {
            Err(anyhow!("No update available"))
        }
    }

    fn get_latest_release(&mut self, ids: &[EventId], cx: &mut Context<Self>) {
        let http_client = cx.http_client();
        let ids = ids.to_vec();

        let task: Task<Result<(InstallerDir, PathBuf), Error>> = cx.background_spawn(async move {
            let client = client();
            let app_pubkey = PublicKey::parse(APP_PUBKEY).unwrap();
            let os = std::env::consts::OS;

            let filter = Filter::new()
                .kind(Kind::FileMetadata)
                .author(app_pubkey)
                .ids(ids);

            // Get all urls for this release
            let events = client.database().query(filter).await?;

            for event in events.into_iter() {
                // Only process events that match current platform
                if event.content != os {
                    continue;
                }

                // Parse the url
                let url = event
                    .tags
                    .find(TagKind::Url)
                    .and_then(|tag| tag.content())
                    .and_then(|content| Url::parse(content).ok())
                    .context("Failed to parse url")?;

                let installer_dir = InstallerDir::new().await?;
                let target_path = Self::target_path(&installer_dir).await?;

                // Download the release
                download(url.as_str(), &target_path, http_client).await?;

                return Ok((installer_dir, target_path));
            }

            Err(anyhow!("Failed to get latest release"))
        });

        self._tasks.push(
            // Install the new release
            cx.spawn(async move |this, cx| {
                this.update(cx, |this, cx| {
                    this.set_status(AutoUpdateStatus::Installing, cx);
                })
                .ok();

                match task.await {
                    Ok((installer_dir, target_path)) => {
                        if Self::install(installer_dir, target_path, cx).await.is_ok() {
                            // Update the status to updated
                            _ = this.update(cx, |this, cx| {
                                this.set_status(AutoUpdateStatus::Updated, cx);
                            });
                        }
                    }
                    Err(e) => {
                        // Update the status to error including the error message
                        _ = this.update(cx, |this, cx| {
                            this.set_status(AutoUpdateStatus::error(e.to_string()), cx);
                        });
                    }
                }

                Ok(())
            }),
        );
    }

    async fn target_path(installer_dir: &InstallerDir) -> Result<PathBuf, Error> {
        let filename = match std::env::consts::OS {
            "macos" => anyhow::Ok("Coop.dmg"),
            "windows" => Ok("Coop.exe"),
            unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
        }?;

        Ok(installer_dir.path().join(filename))
    }

    async fn install(
        installer_dir: InstallerDir,
        target_path: PathBuf,
        cx: &AsyncApp,
    ) -> Result<(), Error> {
        match std::env::consts::OS {
            "macos" => install_release_macos(&installer_dir, target_path, cx).await,
            "windows" => install_release_windows(target_path).await,
            unsupported_os => anyhow::bail!("Not supported: {unsupported_os}"),
        }
    }

    fn set_status(&mut self, status: AutoUpdateStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }
}

async fn download(
    url: &str,
    target_path: &std::path::Path,
    client: Arc<dyn HttpClient>,
) -> Result<(), Error> {
    let body = AsyncBody::default();
    let mut target_file = File::create(&target_path).await?;
    let mut response = client.get(url, body, true).await?;

    // Copy the response body to the target file
    smol::io::copy(response.body_mut(), &mut target_file).await?;

    Ok(())
}

async fn install_release_macos(
    temp_dir: &InstallerDir,
    downloaded_dmg: PathBuf,
    cx: &AsyncApp,
) -> Result<(), Error> {
    let running_app_path = cx.update(|cx| cx.app_path())??;
    let running_app_filename = running_app_path
        .file_name()
        .with_context(|| format!("invalid running app path {running_app_path:?}"))?;

    let mount_path = temp_dir.path().join("Coop");
    let mut mounted_app_path: OsString = mount_path.join(running_app_filename).into();

    mounted_app_path.push("/");

    let output = Command::new("hdiutil")
        .args(["attach", "-nobrowse"])
        .arg(&downloaded_dmg)
        .arg("-mountroot")
        .arg(temp_dir.path())
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to mount: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Create an MacOsUnmounter that will be dropped (and thus unmount the disk) when this function exits
    let _unmounter = MacOsUnmounter {
        mount_path: mount_path.clone(),
        background_executor: cx.background_executor(),
    };

    let output = Command::new("rsync")
        .args(["-av", "--delete"])
        .arg(&mounted_app_path)
        .arg(&running_app_path)
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to copy app: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

async fn install_release_windows(downloaded_installer: PathBuf) -> Result<(), Error> {
    //const CREATE_NO_WINDOW: u32 = 0x08000000;

    let system_root = std::env::var("SYSTEMROOT");
    let powershell_path = system_root.as_ref().map_or_else(
        |_| "powershell.exe".to_string(),
        |p| format!("{p}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"),
    );

    let mut installer_path = std::ffi::OsString::new();
    installer_path.push("\"");
    installer_path.push(&downloaded_installer);
    installer_path.push("\"");

    let output = Command::new(powershell_path)
        //.creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-WindowStyle", "Hidden"])
        .args(["Start-Process"])
        .arg(installer_path)
        .arg("-ArgumentList")
        .args(["/P", "/R"])
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to start installer: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}
