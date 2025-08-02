#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScrollBarMode {
    #[default]
    Scrolling,
    Hover,
    Always,
}

impl ScrollBarMode {
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
