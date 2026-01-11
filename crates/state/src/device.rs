use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DeviceState {
    #[default]
    Initial,
    Requesting,
    Set,
}

/// Announcement
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Announcement {
    /// The public key of the device that created this announcement.
    public_key: PublicKey,

    /// The name of the device that created this announcement.
    client_name: Option<String>,
}

impl From<&Event> for Announcement {
    fn from(val: &Event) -> Self {
        let public_key = val
            .tags
            .iter()
            .find(|tag| tag.kind().as_str() == "n" || tag.kind().as_str() == "P")
            .and_then(|tag| tag.content())
            .and_then(|c| PublicKey::parse(c).ok())
            .unwrap_or(val.pubkey);

        let client_name = val
            .tags
            .find(TagKind::Client)
            .and_then(|tag| tag.content())
            .map(|c| c.to_string());

        Self::new(public_key, client_name)
    }
}

impl Announcement {
    pub fn new(public_key: PublicKey, client_name: Option<String>) -> Self {
        Self {
            public_key,
            client_name,
        }
    }

    /// Returns the public key of the device that created this announcement.
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    /// Returns the client name of the device that created this announcement.
    pub fn client_name(&self) -> SharedString {
        self.client_name
            .as_ref()
            .map(SharedString::from)
            .unwrap_or(SharedString::from("Unknown"))
    }
}
