use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize, Serialize)]
pub enum SignerKind {
    Encryption,
    #[default]
    User,
    Auto,
}
