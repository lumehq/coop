use std::sync::Arc;

use assets::Assets;
use common::{APP_ID, CLIENT_NAME};
use gpui::{
    point, px, size, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem, SharedString,
    TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
    WindowOptions,
};
use ui::Root;

use crate::actions::{load_embedded_fonts, quit, Quit};

pub(crate) mod actions;
pub(crate) mod chatspace;
pub(crate) mod views;

i18n::init!();

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize the Application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    // Run application
    app.run(move |cx| {
        // Load embedded fonts in assets/fonts
        load_embedded_fonts(cx);

        // Register the `quit` function
        cx.on_action(quit);

        // Register the `quit` function with CMD+Q (macOS)
        #[cfg(target_os = "macos")]
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

        // Register the `quit` function with Super+Q (others)
        #[cfg(not(target_os = "macos"))]
        cx.bind_keys([KeyBinding::new("super-q", Quit, None)]);

        // Set menu items
        cx.set_menus(vec![Menu {
            name: "Coop".into(),
            items: vec![MenuItem::action("Quit", Quit)],
        }]);

        // Set up the window bounds
        let bounds = Bounds::centered(None, size(px(920.0), px(700.0)), cx);

        // Set up the window options
        let opts = WindowOptions {
            window_background: WindowBackgroundAppearance::Opaque,
            window_decorations: Some(WindowDecorations::Client),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            kind: WindowKind::Normal,
            app_id: Some(APP_ID.to_owned()),
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::new_static(CLIENT_NAME)),
                traffic_light_position: Some(point(px(9.0), px(9.0))),
                appears_transparent: true,
            }),
            ..Default::default()
        };

        // Open a window with default options
        cx.open_window(opts, |window, cx| {
            // Bring the app to the foreground
            cx.activate(true);

            // Root Entity
            cx.new(|cx| {
                // Initialize the tokio runtime
                gpui_tokio::init(cx);

                // Initialize the nostr client
                nostr::init(cx);

                // Initialize components
                ui::init(cx);

                // Initialize backend for keys storage
                key_store::init(cx);

                // Initialize account state
                account::init(cx);

                // Initialize app registry
                chat::init(cx);

                // Initialize person registry
                person::init(cx);

                // Initialize settings
                settings::init(cx);

                // Initialize auto update
                auto_update::init(cx);

                Root::new(chatspace::init(window, cx).into(), window, cx)
            })
        })
        .expect("Failed to open window. Please restart the application.");
    });
}
