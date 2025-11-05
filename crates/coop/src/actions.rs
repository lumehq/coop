use std::sync::Mutex;

use gpui::{actions, App};
use key_store::{KeyItem, KeyStore};
use nostr_connect::prelude::*;
use state::NostrRegistry;

actions!(coop, [ReloadMetadata, DarkMode, Settings, Logout, Quit]);
actions!(sidebar, [Reload, RelayStatus]);

#[derive(Debug, Clone)]
pub struct CoopAuthUrlHandler;

impl AuthUrlHandler for CoopAuthUrlHandler {
    #[allow(mismatched_lifetime_syntaxes)]
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
    let backend = KeyStore::global(cx).read(cx).backend();
    let client = NostrRegistry::global(cx).read(cx).client();

    cx.spawn(async move |cx| {
        // Remove the signer
        client.unset_signer().await;

        // Delete user's credentials
        backend
            .delete_credentials(&KeyItem::User.to_string(), cx)
            .await
            .ok();

        // Remove bunker's credentials if available
        backend
            .delete_credentials(&KeyItem::Bunker.to_string(), cx)
            .await
            .ok();

        cx.update(|cx| {
            cx.restart();
        })
        .ok();
    })
    .detach();
}

pub fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
