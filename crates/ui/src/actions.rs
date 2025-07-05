use gpui::{actions, Action};
use serde::Deserialize;

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
