use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Announcement {
    id: EventId,
    public_key: PublicKey,
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

        Self::new(val.id, client_name, public_key)
    }
}

impl Announcement {
    pub fn new(id: EventId, client_name: Option<String>, public_key: PublicKey) -> Self {
        Self {
            id,
            client_name,
            public_key,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub fn client_name(&self) -> SharedString {
        self.client_name
            .as_ref()
            .map(SharedString::from)
            .unwrap_or(SharedString::from("Unknown"))
    }
}
