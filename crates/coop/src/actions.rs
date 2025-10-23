use std::sync::Mutex;

use gpui::{actions, App, AppContext};
use key_store::backend::KeyItem;
use key_store::KeyStore;
use nostr_connect::prelude::*;
use registry::Registry;
use states::app_state;

actions!(coop, [ReloadMetadata, DarkMode, Settings, Logout, Quit]);
actions!(sidebar, [Reload, RelayStatus]);

#[derive(Debug, Clone)]
pub struct CoopAuthUrlHandler;

impl AuthUrlHandler for CoopAuthUrlHandler {
    fn on_auth_url(&self, auth_url: Url) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            log::info!("Received Auth URL: {auth_url}");
            webbrowser::open(auth_url.as_str())?;
            Ok(())
        })
    }
}

pub fn load_embedded_fonts(cx: &App) {
    let asset_source = cx.asset_source();
    let font_paths = asset_source.list("fonts").unwrap();
    let embedded_fonts = Mutex::new(Vec::new());
    let executor = cx.background_executor();

    executor.block(executor.scoped(|scope| {
        for font_path in &font_paths {
            if !font_path.ends_with(".ttf") {
                continue;
            }

            scope.spawn(async {
                let font_bytes = asset_source.load(font_path).unwrap().unwrap();
                embedded_fonts.lock().unwrap().push(font_bytes);
            });
        }
    }));

    cx.text_system()
        .add_fonts(embedded_fonts.into_inner().unwrap())
        .unwrap();
}

pub fn reset(cx: &mut App) {
    let registry = Registry::global(cx);
    let backend = KeyStore::global(cx).read(cx).backend();

    cx.spawn(async move |cx| {
        cx.background_spawn(async move {
            let client = app_state().client();
            client.unset_signer().await;
        })
        .await;

        backend
            .delete_credentials(&KeyItem::User.to_string(), cx)
            .await
            .ok();

        backend
            .delete_credentials(&KeyItem::Bunker.to_string(), cx)
            .await
            .ok();

        registry
            .update(cx, |this, cx| {
                this.reset(cx);
            })
            .ok();
    })
    .detach();
}

pub fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
