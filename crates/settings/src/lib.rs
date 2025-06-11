use gpui::{App, AppContext, Context, Entity, Global};
use nostr_sdk::prelude::*;

pub fn init(cx: &mut App) {
    Settings::set_global(cx.new(Settings::new), cx);
}

struct GlobalSettings(Entity<Settings>);

impl Global for GlobalSettings {}

pub struct Settings {
    pub media_server: Url,
    pub proxy_user_avatars: bool,
    pub hide_user_avatars: bool,
    pub only_show_trusted: bool,
    pub backup_messages: bool,
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

    /// Set the global Settings instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalSettings(state));
    }

    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            media_server: Url::parse("https://nostrmedia.com").expect("it's fine"),
            proxy_user_avatars: true,
            hide_user_avatars: false,
            only_show_trusted: false,
            backup_messages: true,
        }
    }
}
