use std::env::consts::OS;
use std::env::{self};
use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{anyhow, Context as _, Error};
use global::shared_state;
use gpui::{App, AppContext, Context, Entity, Global, SemanticVersion, Task};
use nostr_sdk::prelude::*;
use smol::fs::{self, File};
use smol::io::AsyncWriteExt;
use smol::process::Command;
use tempfile::TempDir;

i18n::init!();

struct GlobalAutoUpdate(Entity<AutoUpdater>);

impl Global for GlobalAutoUpdate {}

pub fn init(cx: &mut App) {
    let env = env!("CARGO_PKG_VERSION");
    let current_version: SemanticVersion = env.parse().expect("Invalid version in Cargo.toml");

    AutoUpdater::set_global(
        cx.new(|_| AutoUpdater {
            current_version,
            status: AutoUpdateStatus::Idle,
        }),
        cx,
    );
}

struct MacOsUnmounter {
    mount_path: PathBuf,
}

impl Drop for MacOsUnmounter {
    fn drop(&mut self) {
        let unmount_output = std::process::Command::new("hdiutil")
            .args(["detach", "-force"])
            .arg(&self.mount_path)
            .output();

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
                log::error!("Error while trying to unmount disk image: {error:?}");
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoUpdateStatus {
    Idle,
    Checking,
    Downloading,
    Installing,
    Updated { binary_path: PathBuf },
    Errored,
}

impl AutoUpdateStatus {
    pub fn is_updated(&self) -> bool {
        matches!(self, Self::Updated { .. })
    }
}

#[derive(Debug)]
pub struct AutoUpdater {
    status: AutoUpdateStatus,
    current_version: SemanticVersion,
}

impl AutoUpdater {
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAutoUpdate>().0.clone()
    }

    pub fn set_global(auto_updater: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAutoUpdate(auto_updater));
    }

    pub fn current_version(&self) -> SemanticVersion {
        self.current_version
    }

    pub fn status(&self) -> AutoUpdateStatus {
        self.status.clone()
    }

    pub fn set_status(&mut self, status: AutoUpdateStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    pub fn update(&mut self, event: Event, cx: &mut Context<Self>) {
        self.set_status(AutoUpdateStatus::Checking, cx);

        // Extract the version from the identifier tag
        let ident = match event.tags.identifier() {
            Some(i) => match i.split('@').next_back() {
                Some(i) => i,
                None => return,
            },
            None => return,
        };

        // Convert the version string to a SemanticVersion
        let new_version: SemanticVersion = ident.parse().expect("Invalid version");

        // Check if the new version is the same as the current version
        if self.current_version == new_version {
            self.set_status(AutoUpdateStatus::Idle, cx);
            return;
        };

        // Download the new version
        self.set_status(AutoUpdateStatus::Downloading, cx);

        let task: Task<Result<(TempDir, PathBuf), Error>> = cx.background_spawn(async move {
            let database = shared_state().client().database();
            let ids = event.tags.event_ids().copied();
            let filter = Filter::new().ids(ids).kind(Kind::FileMetadata);
            let events = database.query(filter).await?;

            if let Some(event) = events.into_iter().find(|event| event.content == OS) {
                let tag = event.tags.find(TagKind::Url).context("url not found")?;
                let url = Url::parse(tag.content().context("invalid")?)?;

                let temp_dir = tempfile::Builder::new().prefix("coop-update").tempdir()?;
                let filename = match OS {
                    "macos" => Ok("Coop.dmg"),
                    "linux" => Ok("Coop.tar.gz"),
                    "windows" => Ok("CoopUpdateInstaller.exe"),
                    _ => Err(anyhow!("not supported: {:?}", OS)),
                }?;

                let downloaded_asset = temp_dir.path().join(filename);
                let mut target_file = File::create(&downloaded_asset).await?;

                let response = reqwest::get(url).await?;
                let mut stream = response.bytes_stream();

                while let Some(item) = stream.next().await {
                    let chunk = item?;
                    target_file.write_all(&chunk).await?;
                }

                log::info!("downloaded update. path: {downloaded_asset:?}");

                Ok((temp_dir, downloaded_asset))
            } else {
                Err(anyhow!("Not found"))
            }
        });

        cx.spawn(async move |this, cx| {
            if let Ok((temp_dir, downloaded_asset)) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.set_status(AutoUpdateStatus::Installing, cx);

                        match OS {
                            "macos" => this.install_release_macos(temp_dir, downloaded_asset, cx),
                            "linux" => this.install_release_linux(temp_dir, downloaded_asset, cx),
                            "windows" => this.install_release_windows(downloaded_asset, cx),
                            _ => {}
                        }
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn install_release_macos(&mut self, temp_dir: TempDir, asset: PathBuf, cx: &mut Context<Self>) {
        let running_app_path = cx.app_path().unwrap();
        let running_app_filename = running_app_path.file_name().unwrap();

        let mount_path = temp_dir.path().join("Coop");

        let mut mounted_app_path: OsString = mount_path.join(running_app_filename).into();
        mounted_app_path.push("/");

        let task: Task<Result<PathBuf, Error>> = cx.background_spawn(async move {
            let output = Command::new("hdiutil")
                .args(["attach", "-nobrowse"])
                .arg(&asset)
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

            Ok(running_app_path)
        });

        cx.spawn(async move |this, cx| {
            if let Ok(binary_path) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.status = AutoUpdateStatus::Updated { binary_path };
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn install_release_linux(&mut self, temp_dir: TempDir, asset: PathBuf, cx: &mut Context<Self>) {
        let home_dir = PathBuf::from(env::var("HOME").unwrap());
        let running_app_path = cx.app_path().unwrap();
        let extracted = temp_dir.path().join("coop");

        let task: Task<Result<PathBuf, Error>> = cx.background_spawn(async move {
            fs::create_dir_all(&extracted).await?;

            let output = Command::new("tar")
                .arg("-xzf")
                .arg(&asset)
                .arg("-C")
                .arg(&extracted)
                .output()
                .await?;

            anyhow::ensure!(
                output.status.success(),
                "failed to extract {:?} to {:?}: {:?}",
                asset,
                extracted,
                String::from_utf8_lossy(&output.stderr)
            );

            let app_folder_name: String = "coop.app".into();
            let from = extracted.join(&app_folder_name);
            let mut to = home_dir.join(".local");

            let expected_suffix = format!("{app_folder_name}/libexec/coop");

            if let Some(prefix) = running_app_path
                .to_str()
                .and_then(|str| str.strip_suffix(&expected_suffix))
            {
                to = PathBuf::from(prefix);
            }

            let output = Command::new("rsync")
                .args(["-av", "--delete"])
                .arg(&from)
                .arg(&to)
                .output()
                .await?;

            anyhow::ensure!(
                output.status.success(),
                "failed to copy Coop update from {:?} to {:?}: {:?}",
                from,
                to,
                String::from_utf8_lossy(&output.stderr)
            );

            Ok(to.join(expected_suffix))
        });

        cx.spawn(async move |this, cx| {
            if let Ok(binary_path) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.status = AutoUpdateStatus::Updated { binary_path };
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn install_release_windows(&mut self, asset: PathBuf, cx: &mut Context<Self>) {
        let task: Task<Result<PathBuf, Error>> = cx.background_spawn(async move {
            let output = Command::new(asset)
                .arg("/verysilent")
                .arg("/update=true")
                .arg("!desktopicon")
                .arg("!quicklaunchicon")
                .output()
                .await?;
            anyhow::ensure!(
                output.status.success(),
                "failed to start installer: {:?}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(std::env::current_exe()?)
        });

        cx.spawn(async move |this, cx| {
            if let Ok(binary_path) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.status = AutoUpdateStatus::Updated { binary_path };
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }
}
