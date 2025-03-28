use std::path::PathBuf;
use std::sync::OnceLock;

/// Returns the path to the user's home directory.
pub fn home_dir() -> &'static PathBuf {
    static HOME_DIR: OnceLock<PathBuf> = OnceLock::new();
    HOME_DIR.get_or_init(|| dirs::home_dir().expect("failed to determine home directory"))
}

/// Returns the path to the configuration directory used by Coop.
pub fn config_dir() -> &'static PathBuf {
    static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
    CONFIG_DIR.get_or_init(|| {
        if cfg!(target_os = "windows") {
            return dirs::config_dir()
                .expect("failed to determine RoamingAppData directory")
                .join("Coop");
        }

        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            return if let Ok(flatpak_xdg_config) = std::env::var("FLATPAK_XDG_CONFIG_HOME") {
                flatpak_xdg_config.into()
            } else {
                dirs::config_dir().expect("failed to determine XDG_CONFIG_HOME directory")
            }
            .join("coop");
        }

        home_dir().join(".config").join("coop")
    })
}

/// Returns the path to the support directory used by Coop.
pub fn support_dir() -> &'static PathBuf {
    static SUPPORT_DIR: OnceLock<PathBuf> = OnceLock::new();
    SUPPORT_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            return home_dir().join("Library/Application Support/Coop");
        }

        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            return if let Ok(flatpak_xdg_data) = std::env::var("FLATPAK_XDG_DATA_HOME") {
                flatpak_xdg_data.into()
            } else {
                dirs::data_local_dir().expect("failed to determine XDG_DATA_HOME directory")
            }
            .join("coop");
        }

        if cfg!(target_os = "windows") {
            return dirs::data_local_dir()
                .expect("failed to determine LocalAppData directory")
                .join("coop");
        }

        config_dir().clone()
    })
}

/// Returns the path to the `nostr` file.
pub fn nostr_file() -> &'static PathBuf {
    static NOSTR_FILE: OnceLock<PathBuf> = OnceLock::new();
    NOSTR_FILE.get_or_init(|| support_dir().join("nostr"))
}
