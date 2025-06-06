use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use asset::Assets;
use auto_update::AutoUpdater;
use chats::ChatRegistry;
#[cfg(not(target_os = "linux"))]
use global::constants::APP_NAME;
use global::constants::{APP_ID, KEYRING_BUNKER, KEYRING_USER_PATH};
use global::{shared_state, NostrSignal};
use gpui::{
    actions, px, size, App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem,
    WindowBounds, WindowKind, WindowOptions,
};
#[cfg(not(target_os = "linux"))]
use gpui::{point, SharedString, TitlebarOptions};
#[cfg(target_os = "linux")]
use gpui::{WindowBackgroundAppearance, WindowDecorations};
use nostr_connect::prelude::*;
use theme::Theme;
use ui::Root;

pub(crate) mod asset;
pub(crate) mod chatspace;
pub(crate) mod views;

actions!(coop, [Quit]);

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize the Global State and process events in a separate thread.
    // Must be run under async utility runtime
    nostr_sdk::async_utility::task::spawn(async move {
        shared_state().start().await;
    });

    // Initialize the Application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    app.run(move |cx| {
        // Register the `quit` function
        cx.on_action(quit);

        // Register the `quit` function with CMD+Q (macOS only)
        #[cfg(target_os = "macos")]
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

        // Set menu items
        cx.set_menus(vec![Menu {
            name: "Coop".into(),
            items: vec![MenuItem::action("Quit", Quit)],
        }]);

        // Set up the window options
        let opts = WindowOptions {
            #[cfg(not(target_os = "linux"))]
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::new_static(APP_NAME)),
                traffic_light_position: Some(point(px(9.0), px(9.0))),
                appears_transparent: true,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(920.0), px(700.0)),
                cx,
            ))),
            #[cfg(target_os = "linux")]
            window_background: WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(WindowDecorations::Client),
            kind: WindowKind::Normal,
            app_id: Some(APP_ID.to_owned()),
            ..Default::default()
        };

        // Open a window with default options
        cx.open_window(opts, |window, cx| {
            // Automatically sync theme with system appearance
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            // Root Entity
            cx.new(|cx| {
                cx.activate(true);
                // Initialize components
                ui::init(cx);
                // Initialize auto update
                auto_update::init(cx);
                // Initialize chat state
                chats::init(cx);

                // Initialize chatspace (or workspace)
                let chatspace = chatspace::init(window, cx);
                let async_chatspace = chatspace.downgrade();
                let async_chatspace_clone = async_chatspace.clone();

                // Read user's credential
                let read_credential = cx.read_credentials(KEYRING_USER_PATH);

                cx.spawn_in(window, async move |_, cx| {
                    if let Ok(Some((user, secret))) = read_credential.await {
                        cx.update(|window, cx| {
                            if let Ok(signer) = extract_credential(&user, secret) {
                                cx.background_spawn(async move {
                                    if let Err(e) = shared_state().set_signer(signer).await {
                                        log::error!("Signer error: {}", e);
                                    }
                                })
                                .detach();
                            } else {
                                async_chatspace
                                    .update(cx, |this, cx| {
                                        this.open_onboarding(window, cx);
                                    })
                                    .ok();
                            }
                        })
                        .ok();
                    } else {
                        cx.update(|window, cx| {
                            async_chatspace
                                .update(cx, |this, cx| {
                                    this.open_onboarding(window, cx);
                                })
                                .ok();
                        })
                        .ok();
                    }
                })
                .detach();

                // Spawn a task to handle events from nostr channel
                cx.spawn_in(window, async move |_, cx| {
                    while let Ok(signal) = shared_state().global_receiver.recv().await {
                        cx.update(|window, cx| {
                            let chats = ChatRegistry::global(cx);
                            let auto_updater = AutoUpdater::global(cx);

                            match signal {
                                NostrSignal::SignerUpdated => {
                                    async_chatspace_clone
                                        .update(cx, |this, cx| {
                                            this.open_chats(window, cx);
                                        })
                                        .ok();
                                }
                                NostrSignal::Eose => {
                                    chats.update(cx, |this, cx| {
                                        this.load_rooms(window, cx);
                                    });
                                }
                                NostrSignal::Event(event) => {
                                    chats.update(cx, |this, cx| {
                                        this.event_to_message(event, window, cx);
                                    });
                                }
                                NostrSignal::AppUpdate(event) => {
                                    auto_updater.update(cx, |this, cx| {
                                        this.update(event, cx);
                                    });
                                }
                            };
                        })
                        .ok();
                    }
                })
                .detach();

                Root::new(chatspace.into(), window, cx)
            })
        })
        .expect("Failed to open window. Please restart the application.");
    });
}

fn extract_credential(user: &str, secret: Vec<u8>) -> Result<impl NostrSigner, Error> {
    if user == KEYRING_BUNKER {
        let value = String::from_utf8(secret)?;
        let uri = NostrConnectURI::parse(value)?;
        let client_keys = shared_state().client_signer.clone();
        let signer = NostrConnect::new(uri, client_keys, Duration::from_secs(300), None)?;

        Ok(signer.into_nostr_signer())
    } else {
        let secret_key = SecretKey::from_slice(&secret)?;
        let keys = Keys::new(secret_key);

        Ok(keys.into_nostr_signer())
    }
}

fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
