use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyItem {
    Encryption,
    Client,
    Bunker,
    User,
}

impl Display for KeyItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyItem::Encryption => write!(f, "encryption"),
            KeyItem::Client => write!(f, "client"),
            KeyItem::Bunker => write!(f, "bunker"),
            KeyItem::User => write!(f, "user"),
        }
    }
}

impl From<KeyItem> for String {
    fn from(val: KeyItem) -> Self {
        val.to_string()
    }
}
