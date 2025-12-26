use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
pub enum ScrollbarMode {
    #[default]
    Scrolling,
    Hover,
    Always,
}

impl ScrollbarMode {
    pub fn is_scrolling(&self) -> bool {
        matches!(self, Self::Scrolling)
    }

    pub fn is_hover(&self) -> bool {
        matches!(self, Self::Hover)
    }

    pub fn is_always(&self) -> bool {
        matches!(self, Self::Always)
    }
}
