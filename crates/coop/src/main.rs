use std::sync::Arc;

use asset::Assets;
use auto_update::AutoUpdater;
use chats::ChatRegistry;
#[cfg(not(target_os = "linux"))]
use global::constants::APP_NAME;
use global::constants::{ALL_MESSAGES_SUB_ID, APP_ID, APP_NAME};
use global::{shared_state, NostrSignal};
#[cfg(target_os = "linux")]
use gpui::WindowDecorations;
use gpui::{
    actions, point, px, size, App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem,
    SharedString, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions,
};
use nostr_sdk::SubscriptionId;
use theme::Theme;
use ui::Root;

pub(crate) mod asset;
pub(crate) mod chatspace;
pub(crate) mod views;

actions!(coop, [Quit]);

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize the Application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    // Initialize the Global State and process events in a separate thread.
    app.background_executor()
        .spawn(async move {
            shared_state().start().await;
        })
        .detach();

    // Run application
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
            window_background: WindowBackgroundAppearance::Opaque,
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
                // Initialize settings
                settings::init(cx);
                // Initialize client keys
                client_keys::init(cx);
                // Initialize identity
                identity::init(window, cx);
                // Initialize auto update
                auto_update::init(cx);
                // Initialize chat state
                chats::init(cx);

                // Spawn a task to handle events from nostr channel
                cx.spawn_in(window, async move |_, cx| {
                    let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);

                    while let Ok(signal) = shared_state().signal().recv().await {
                        cx.update(|window, cx| {
                            let chats = ChatRegistry::global(cx);
                            let auto_updater = AutoUpdater::global(cx);

                            match signal {
                                NostrSignal::Event(event) => {
                                    chats.update(cx, |this, cx| {
                                        this.event_to_message(event, window, cx);
                                    });
                                }
                                // Load chat rooms and stop the loading status
                                NostrSignal::Finish => {
                                    chats.update(cx, |this, cx| {
                                        this.load_rooms(window, cx);
                                        this.set_loading(false, cx);
                                    });
                                }
                                // Load chat rooms without setting as finished
                                NostrSignal::PartialFinish => {
                                    chats.update(cx, |this, cx| {
                                        this.load_rooms(window, cx);
                                    });
                                }
                                NostrSignal::Eose(subscription_id) => {
                                    if subscription_id == all_messages_sub_id {
                                        chats.update(cx, |this, cx| {
                                            this.load_rooms(window, cx);
                                        });
                                    }
                                }
                                NostrSignal::Notice(_msg) => {
                                    // window.push_notification(msg, cx);
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

                Root::new(chatspace::init(window, cx).into(), window, cx)
            })
        })
        .expect("Failed to open window. Please restart the application.");
    });
}

fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
