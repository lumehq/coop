#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum PlatformKind {
    Mac,
    Linux,
    Windows,
}

impl PlatformKind {
    pub const fn platform() -> Self {
        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Mac
        }
    }

    #[allow(dead_code)]
    pub fn is_linux(&self) -> bool {
        matches!(self, Self::Linux)
    }

    #[allow(dead_code)]
    pub fn is_windows(&self) -> bool {
        matches!(self, Self::Windows)
    }

    #[allow(dead_code)]
    pub fn is_mac(&self) -> bool {
        matches!(self, Self::Mac)
    }
}
