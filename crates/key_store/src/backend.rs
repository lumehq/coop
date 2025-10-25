use std::any::Any;
use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use anyhow::Result;
use futures::FutureExt as _;
use gpui::AsyncApp;
use states::paths::config_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyItem {
    User,
    Bunker,
}

impl Display for KeyItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "coop-user"),
            Self::Bunker => write!(f, "coop-bunker"),
        }
    }
}

impl From<KeyItem> for String {
    fn from(item: KeyItem) -> Self {
        item.to_string()
    }
}

pub trait KeyBackend: Any + Send + Sync {
    fn name(&self) -> &str;

    /// Reads the credentials from the provider.
    #[allow(clippy::type_complexity)]
    fn read_credentials<'a>(
        &'a self,
        url: &'a str,
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, Vec<u8>)>>> + 'a>>;

    /// Writes the credentials to the provider.
    fn write_credentials<'a>(
        &'a self,
        url: &'a str,
        username: &'a str,
        password: &'a [u8],
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;

    /// Deletes the credentials from the provider.
    fn delete_credentials<'a>(
        &'a self,
        url: &'a str,
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;
}

/// A credentials provider that stores credentials in the system keychain.
pub struct KeyringProvider;

impl KeyBackend for KeyringProvider {
    fn name(&self) -> &str {
        "keyring"
    }

    fn read_credentials<'a>(
        &'a self,
        url: &'a str,
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, Vec<u8>)>>> + 'a>> {
        async move { cx.update(|cx| cx.read_credentials(url))?.await }.boxed_local()
    }

    fn write_credentials<'a>(
        &'a self,
        url: &'a str,
        username: &'a str,
        password: &'a [u8],
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        async move {
            cx.update(move |cx| cx.write_credentials(url, username, password))?
                .await
        }
        .boxed_local()
    }

    fn delete_credentials<'a>(
        &'a self,
        url: &'a str,
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        async move { cx.update(move |cx| cx.delete_credentials(url))?.await }.boxed_local()
    }
}

/// A credentials provider that stores credentials in a local file.
pub struct FileProvider {
    path: PathBuf,
}

impl FileProvider {
    pub fn new() -> Self {
        let path = config_dir().join(".keys");

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        Self { path }
    }

    pub fn load_credentials(&self) -> Result<HashMap<String, (String, Vec<u8>)>> {
        let json = std::fs::read(&self.path)?;
        let credentials: HashMap<String, (String, Vec<u8>)> = serde_json::from_slice(&json)?;

        Ok(credentials)
    }

    pub fn save_credentials(&self, credentials: &HashMap<String, (String, Vec<u8>)>) -> Result<()> {
        let json = serde_json::to_string(credentials)?;
        std::fs::write(&self.path, json)?;

        Ok(())
    }
}

impl Default for FileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyBackend for FileProvider {
    fn name(&self) -> &str {
        "file"
    }

    fn read_credentials<'a>(
        &'a self,
        url: &'a str,
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, Vec<u8>)>>> + 'a>> {
        async move {
            Ok(self
                .load_credentials()
                .unwrap_or_default()
                .get(url)
                .cloned())
        }
        .boxed_local()
    }

    fn write_credentials<'a>(
        &'a self,
        url: &'a str,
        username: &'a str,
        password: &'a [u8],
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        async move {
            let mut credentials = self.load_credentials().unwrap_or_default();
            credentials.insert(url.to_string(), (username.to_string(), password.to_vec()));

            self.save_credentials(&credentials)
        }
        .boxed_local()
    }

    fn delete_credentials<'a>(
        &'a self,
        url: &'a str,
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        async move {
            let mut credentials = self.load_credentials()?;
            credentials.remove(url);

            self.save_credentials(&credentials)
        }
        .boxed_local()
    }
}
