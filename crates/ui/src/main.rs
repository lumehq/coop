use client::NostrClient;
use components::theme::{Theme, ThemeColor, ThemeMode};
use gpui::*;
use state::AppState;
use views::app::AppView;

pub mod state;
pub mod utils;
pub mod views;

#[tokio::main]
async fn main() {
    // Initialize nostr client
    let nostr = NostrClient::init().await;
    // Initializ app state
    let app_state = AppState::new();

    App::new().run(move |cx| {
        // Initialize components
        components::init(cx);

        // Set custom theme
        let mut theme = Theme::from(ThemeColor::dark());
        // TODO: support light mode
        theme.mode = ThemeMode::Dark;
        // TODO: adjust color set

        // Set global theme
        cx.set_global(theme);

        // Set nostr client as global state
        cx.set_global(nostr);
        cx.set_global(app_state);

        // Rerender
        cx.refresh();

        // Set window size
        let bounds = Bounds::centered(None, size(px(860.0), px(650.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::new_static("coop")),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |cx| cx.new_view(AppView::new),
        )
        .unwrap();
    });
}
