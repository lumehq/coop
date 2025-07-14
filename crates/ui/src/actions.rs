use gpui::{actions, Action};
use nostr_sdk::prelude::PublicKey;
use serde::Deserialize;

/// Define a open profile action
#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = profile, no_json)]
pub struct OpenProfile(pub PublicKey);

/// Define a custom confirm action
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = list, no_json)]
pub struct Confirm {
    /// Is confirm with secondary.
    pub secondary: bool,
}

actions!(
    list,
    [
        /// Close current list
        Cancel,
        /// Select the next item in lists
        SelectPrev,
        /// Select the previous item in list
        SelectNext
    ]
);
