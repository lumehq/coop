use asset::Assets;
use components::Root;
use constants::{APP_NAME, FAKE_SIG};
use dirs::config_dir;
use gpui::*;
use nostr_sdk::prelude::*;
use std::{fs, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::OnceCell;

use states::user::UserState;
use ui::app::AppView;

pub mod asset;
pub mod constants;
pub mod states;
pub mod ui;
pub mod utils;

actions!(main_menu, [Quit]);

pub static CLIENT: OnceCell<Client> = OnceCell::const_new();

pub async fn get_client() -> &'static Client {
    CLIENT
        .get_or_init(|| async {
            // Setup app data folder
            let config_dir = config_dir().expect("Config directory not found");
            let _ = fs::create_dir_all(config_dir.join("Coop/"));

            // Setup database
            let lmdb = NostrLMDB::open(config_dir.join("Coop/nostr"))
                .expect("Database is NOT initialized");

            // Client options
            let opts = Options::new()
                .gossip(true)
                .max_avg_latency(Duration::from_secs(2));

            // Setup Nostr Client
            let client = ClientBuilder::default().database(lmdb).opts(opts).build();

            // Add some bootstrap relays
            let _ = client.add_relay("wss://relay.damus.io").await;
            let _ = client.add_relay("wss://relay.primal.net").await;

            let _ = client.add_discovery_relay("wss://directory.yabu.me").await;
            let _ = client.add_discovery_relay("wss://user.kindpag.es/").await;

            // Connect to all relays
            client.connect().await;

            // Return client
            client
        })
        .await
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Initialize nostr client
    let _client = get_client().await;

    App::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()))
        .run(move |cx| {
            // Initialize components
            components::init(cx);

            // Set quit action
            cx.on_action(quit);

            // Set app state
            UserState::set_global(cx);

            // Refresh
            cx.refresh();

            // Handle notifications
            cx.foreground_executor()
                .spawn(async move {
                    let client = get_client().await;

                    // Generate a fake signature for rumor event.
                    // TODO: Find better way to save unsigned event to database.
                    let fake_sig = Signature::from_str(FAKE_SIG).unwrap();

                    client
                        .handle_notifications(|notification| async {
                            #[allow(clippy::collapsible_match)]
                            if let RelayPoolNotification::Message { message, .. } = notification {
                                if let RelayMessage::Event { event, .. } = message {
                                    if event.kind == Kind::GiftWrap {
                                        if let Ok(UnwrappedGift { rumor, .. }) =
                                            client.unwrap_gift_wrap(&event).await
                                        {
                                            println!("rumor: {}", rumor.as_json());
                                            let mut rumor_clone = rumor.clone();

                                            // Compute event id if not exist
                                            rumor_clone.ensure_id();

                                            let ev = Event::new(
                                                rumor_clone.id.expect("System error"),
                                                rumor_clone.pubkey,
                                                rumor_clone.created_at,
                                                rumor_clone.kind,
                                                rumor_clone.tags,
                                                rumor_clone.content,
                                                fake_sig,
                                            );

                                            // Save rumor to database to further query
                                            if let Err(e) = client.database().save_event(&ev).await
                                            {
                                                println!("Error: {}", e)
                                            }
                                        }
                                    } else if event.kind == Kind::Metadata {
                                        // TODO: handle metadata
                                    }
                                }
                            }
                            Ok(false)
                        })
                        .await
                })
                .detach();

            // Set window size
            let bounds = Bounds::centered(None, size(px(900.0), px(680.0)), cx);

            let opts = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::new_static(APP_NAME)),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(9.0), px(9.0))),
                }),
                ..Default::default()
            };

            cx.open_window(opts, |cx| {
                let app_view = cx.new_view(AppView::new);
                cx.new_view(|cx| Root::new(app_view.into(), cx))
            })
            .unwrap();
        });
}

fn quit(_: &Quit, cx: &mut AppContext) {
    cx.quit();
}
