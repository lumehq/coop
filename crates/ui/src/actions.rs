use gpui::{actions, Action};
use nostr_sdk::prelude::PublicKey;
use serde::Deserialize;

/// Define a open public key action
#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = pubkey, no_json)]
pub struct OpenPublicKey(pub PublicKey);

/// Define a copy inline public key action
#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = pubkey, no_json)]
pub struct CopyPublicKey(pub PublicKey);

/// Define a custom confirm action
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = list, no_json)]
pub struct Confirm {
    /// Is confirm with secondary.
    pub secondary: bool,
}

actions!(ui, [Cancel, SelectUp, SelectDown, SelectLeft, SelectRight]);
