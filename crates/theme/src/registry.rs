use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Error};
use gpui::{App, AppContext, AssetSource, Context, Entity, Global, SharedString};

use crate::ThemeFamily;

pub fn init(cx: &mut App) {
    ThemeRegistry::set_global(cx.new(ThemeRegistry::new), cx);
}

struct GlobalThemeRegistry(Entity<ThemeRegistry>);

impl Global for GlobalThemeRegistry {}

pub struct ThemeRegistry {
    /// Map of theme names to theme families
    themes: HashMap<SharedString, Rc<ThemeFamily>>,
}

impl ThemeRegistry {
    /// Retrieve the global theme registry state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalThemeRegistry>().0.clone()
    }

    /// Set the global theme registry instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalThemeRegistry(state));
    }

    /// Create a new theme registry instance
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let mut themes = HashMap::new();
        let asset = cx.asset_source();

        if let Ok(paths) = asset.list("themes") {
            for path in paths.into_iter() {
                match Self::load(&path, asset) {
                    Ok(theme) => {
                        themes.insert(path, Rc::new(theme));
                    }
                    Err(e) => {
                        log::error!("Failed to load theme: {path}. Error: {e}");
                    }
                }
            }
        }

        Self { themes }
    }

    /// Load a theme from the asset source.
    fn load(path: &str, asset: &Arc<dyn AssetSource>) -> Result<ThemeFamily, Error> {
        // Load the theme file from the assets
        let content = asset.load(path)?.context("Theme not found")?;

        // Parse the JSON content into a Theme Family struct
        let theme: ThemeFamily = serde_json::from_slice(&content)?;

        Ok(theme)
    }

    /// Returns a reference to the map of themes.
    pub fn themes(&self) -> &HashMap<SharedString, Rc<ThemeFamily>> {
        &self.themes
    }
}
