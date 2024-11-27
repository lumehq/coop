use asset::Assets;
use client::NostrClient;
use constants::{APP_NAME, KEYRING_SERVICE};
use gpui::*;
use keyring::Entry;
use nostr_sdk::prelude::*;
use state::AppState;
use std::sync::Arc;
use utils::get_all_accounts_from_keyring;
use views::app::AppView;

pub mod asset;
pub mod constants;
pub mod state;
pub mod utils;
pub mod views;

actions!(main_menu, [Quit]);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Initialize nostr client
    let nostr = NostrClient::init().await;
    // Initialize app state
    let app_state = AppState::new();

    App::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()))
        .run(move |cx| {
            // Initialize components
            components::init(cx);

            // Set global state
            cx.set_global(nostr);
            cx.set_global(app_state);

            // Set quit action
            cx.on_action(quit);

            // Refresh
            cx.refresh();

            // Login
            let async_cx = cx.to_async();
            cx.foreground_executor()
                .spawn(async move {
                    let accounts = get_all_accounts_from_keyring();

                    if let Some(account) = accounts.first() {
                        let client = async_cx
                            .read_global(|nostr: &NostrClient, _cx| nostr.client)
                            .unwrap();
                        let entry =
                            Entry::new(KEYRING_SERVICE, account.to_bech32().unwrap().as_ref())
                                .unwrap();
                        let password = entry.get_password().unwrap();
                        let keys = Keys::parse(password).unwrap();

                        client.set_signer(keys).await;

                        async_cx
                            .update_global(|app_state: &mut AppState, _cx| {
                                app_state.signer = Some(*account);
                            })
                            .unwrap();
                    }
                })
                .detach();

            // Set window size
            let bounds = Bounds::centered(None, size(px(900.0), px(680.0)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_decorations: Some(WindowDecorations::Client),
                    titlebar: Some(TitlebarOptions {
                        title: Some(SharedString::new_static(APP_NAME)),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(9.0), px(9.0))),
                    }),
                    ..Default::default()
                },
                |cx| cx.new_view(AppView::new),
            )
            .unwrap();
        });
}

fn quit(_: &Quit, cx: &mut AppContext) {
    cx.quit();
}
