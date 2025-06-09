use gpui::{App, Entity, Global};

struct GlobalSettings(Entity<Settings>);

impl Global for GlobalSettings {}

pub struct Settings {
    //
}

impl Settings {
    /// Retrieve the Global Settings instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalSettings>().0.clone()
    }

    /// Retrieve the Settings instance
    pub fn get_global(cx: &App) -> &Self {
        cx.global::<GlobalSettings>().0.read(cx)
    }

    fn new(cx: &App) -> Self {
        Self {}
    }
}
