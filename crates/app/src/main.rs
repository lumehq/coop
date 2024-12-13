use asset::Assets;
use coop_ui::Root;
use dirs::config_dir;
use gpui::*;
use nostr_sdk::prelude::*;
use std::{
    fs,
    str::FromStr,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::sync::mpsc;

use constants::{ALL_MESSAGES_SUB_ID, APP_NAME, FAKE_SIG, NEW_MESSAGE_SUB_ID};
use states::{account::AccountRegistry, chat::ChatRegistry, signal::SignalRegistry};
use views::app::AppView;

pub mod asset;
pub mod constants;
pub mod states;
pub mod utils;
pub mod views;

actions!(main_menu, [Quit]);

static CLIENT: OnceLock<Client> = OnceLock::new();

pub fn initialize_client() {
    // Setup app data folder
    let config_dir = config_dir().expect("Config directory not found");
    let _ = fs::create_dir_all(config_dir.join("Coop/"));

    // Setup database
    let lmdb = NostrLMDB::open(config_dir.join("Coop/nostr")).expect("Database is NOT initialized");

    // Client options
    let opts = Options::new()
        .gossip(true)
        .max_avg_latency(Duration::from_secs(2));

    // Setup Nostr Client
    let client = ClientBuilder::default().database(lmdb).opts(opts).build();

    CLIENT.set(client).expect("Client is already initialized!");
}

pub fn get_client() -> &'static Client {
    CLIENT.get().expect("Client is NOT initialized!")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Initialize client
    initialize_client();

    // Get client
    let client = get_client();
    let mut notifications = client.notifications();

    // Add some bootstrap relays
    _ = client.add_relay("wss://relay.damus.io/").await;
    _ = client.add_relay("wss://relay.primal.net/").await;
    _ = client.add_relay("wss://nos.lol/").await;

    _ = client.add_discovery_relay("wss://user.kindpag.es/").await;
    _ = client.add_discovery_relay("wss://directory.yabu.me/").await;

    // Connect to all relays
    _ = client.connect().await;

    // Channel for metadata signal
    let (signal_tx, mut signal_rx) = mpsc::channel::<PublicKey>(1000); // TODO: adjust?

    // Channel for new chat
    let (new_chat_tx, mut new_chat_rx) = mpsc::channel::<Event>(1000); // TODO: adjust?

    // Channel for all chats
    let (all_chats_tx, mut all_chats_rx) = mpsc::channel::<i32>(1);

    tokio::spawn(async move {
        let sig = Signature::from_str(FAKE_SIG).unwrap();
        let all_messages_sub_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
        let new_message_sub_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

        while let Ok(notification) = notifications.recv().await {
            #[allow(clippy::collapsible_match)]
            if let RelayPoolNotification::Message { message, .. } = notification {
                if let RelayMessage::Event {
                    event,
                    subscription_id,
                } = message
                {
                    if event.kind == Kind::GiftWrap {
                        if let Ok(UnwrappedGift { rumor, .. }) =
                            client.unwrap_gift_wrap(&event).await
                        {
                            let mut rumor_clone = rumor.clone();

                            // Compute event id if not exist
                            rumor_clone.ensure_id();

                            if let Some(id) = rumor_clone.id {
                                let ev = Event::new(
                                    id,
                                    rumor_clone.pubkey,
                                    rumor_clone.created_at,
                                    rumor_clone.kind,
                                    rumor_clone.tags,
                                    rumor_clone.content,
                                    sig,
                                );

                                // Save rumor to database to further query
                                _ = client.database().save_event(&ev).await;

                                // Send event to channel
                                if subscription_id == new_message_sub_id {
                                    _ = new_chat_tx.send(ev).await;
                                }
                            }
                        }
                    } else if event.kind == Kind::Metadata {
                        _ = signal_tx.send(event.pubkey).await;
                    }
                } else if let RelayMessage::EndOfStoredEvents(subscription_id) = message {
                    if all_messages_sub_id == subscription_id {
                        _ = all_chats_tx.send(1).await;
                    }
                }
            }
        }
    });

    App::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()))
        .run(move |cx| {
            AccountRegistry::set_global(cx);
            ChatRegistry::set_global(cx);
            SignalRegistry::set_global(cx);

            // Initialize components
            coop_ui::init(cx);

            // Set quit action
            cx.on_action(quit);

            cx.spawn(|async_cx| async move {
                while let Some(public_key) = signal_rx.recv().await {
                    _ = async_cx.update_global::<SignalRegistry, _>(|state, _cx| {
                        state.push(public_key);
                    });
                }
            })
            .detach();

            cx.spawn(|async_cx| async move {
                while let Some(event) = new_chat_rx.recv().await {
                    _ = async_cx.update_global::<ChatRegistry, _>(|state, cx| {
                        state.push(event, cx);
                    });
                }
            })
            .detach();

            cx.spawn(|async_cx| async move {
                while let Some(_n) = all_chats_rx.recv().await {
                    _ = async_cx.update_global::<ChatRegistry, _>(|state, cx| {
                        state.load(cx);
                    });
                }
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

                cx.activate(true);
                cx.new_view(|cx| Root::new(app_view.into(), cx))
            })
            .unwrap();
        });
}

fn quit(_: &Quit, cx: &mut AppContext) {
    cx.quit();
}
