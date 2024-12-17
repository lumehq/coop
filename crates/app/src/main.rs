use asset::Assets;
use coop_ui::Root;
use dirs::config_dir;
use gpui::*;
use nostr_sdk::prelude::*;
use std::{
    collections::HashSet,
    fs,
    str::FromStr,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{sync::mpsc, time::sleep};

use constants::{ALL_MESSAGES_SUB_ID, APP_NAME, FAKE_SIG, METADATA_DELAY, NEW_MESSAGE_SUB_ID};
use states::{
    account::AccountRegistry,
    chat::ChatRegistry,
    metadata::{MetadataRegistry, Signal},
};
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

    // Channel for EOSE
    // When receive EOSE from relay(s) -> Load all rooms and push it into UI.
    let (eose_tx, mut eose_rx) = mpsc::channel::<SubscriptionId>(200);

    // Channel for new message
    // Push new message to chat panel or create new chat room if not exist.
    let (message_tx, message_rx) = flume::unbounded::<Event>();
    let message_rx_clone = message_rx.clone();

    // Channel for signal
    // Merge all metadata requests into single one.
    // Notify to reload element if receive new metadata.
    let (signal_tx, mut signal_rx) = mpsc::channel::<Signal>(5000);
    let signal_tx_clone = signal_tx.clone();

    tokio::spawn(async move {
        let sig = Signature::from_str(FAKE_SIG).unwrap();
        let new_message = SubscriptionId::new(NEW_MESSAGE_SUB_ID);

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
                                if let Err(e) = client.database().save_event(&ev).await {
                                    println!("Save error: {}", e);
                                }

                                // Send event back to channel
                                if subscription_id == new_message {
                                    if let Err(e) = message_tx.send_async(ev).await {
                                        println!("Error: {}", e)
                                    }
                                }
                            }
                        }
                    } else if event.kind == Kind::Metadata {
                        if let Err(e) = signal_tx.send(Signal::DONE(event.pubkey)).await {
                            println!("Error: {}", e)
                        }
                    }
                } else if let RelayMessage::EndOfStoredEvents(subscription_id) = message {
                    if let Err(e) = eose_tx.send(subscription_id).await {
                        println!("Error: {}", e)
                    }
                }
            }
        }
    });

    App::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()))
        .run(move |cx| {
            // Account state
            AccountRegistry::set_global(cx);
            // Metadata state
            MetadataRegistry::set_global(cx, signal_tx_clone);
            // Chat state
            ChatRegistry::set_global(cx, message_rx);

            // Initialize components
            coop_ui::init(cx);

            // Set quit action
            cx.on_action(quit);

            cx.spawn(|async_cx| async move {
                let mut queue: HashSet<PublicKey> = HashSet::new();

                while let Some(signal) = signal_rx.recv().await {
                    match signal {
                        Signal::REQ(public_key) => {
                            queue.insert(public_key);

                            // Wait for METADATA_DELAY
                            sleep(Duration::from_millis(METADATA_DELAY)).await;

                            if !queue.is_empty() {
                                let authors: Vec<PublicKey> = queue.iter().copied().collect();
                                let total = authors.len();

                                let filter = Filter::new()
                                    .authors(authors)
                                    .kind(Kind::Metadata)
                                    .limit(total);

                                let opts = SubscribeAutoCloseOptions::default().filter(
                                    FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(2)),
                                );

                                queue.clear();

                                async_cx
                                    .background_executor()
                                    .spawn(async move {
                                        if let Err(e) =
                                            client.subscribe(vec![filter], Some(opts)).await
                                        {
                                            println!("Error: {}", e);
                                        }
                                    })
                                    .await;
                            }
                        }
                        Signal::DONE(public_key) => {
                            _ = async_cx.update_global::<MetadataRegistry, _>(|state, _| {
                                state.seen(public_key);
                            });
                        }
                    }
                }
            })
            .detach();

            cx.spawn(|async_cx| async move {
                while let Ok(event) = message_rx_clone.recv_async().await {
                    _ = async_cx.update_global::<ChatRegistry, _>(|state, cx| {
                        state.push(event, cx);
                    });
                }
            })
            .detach();

            cx.spawn(|async_cx| async move {
                let all_messages = SubscriptionId::new(ALL_MESSAGES_SUB_ID);

                while let Some(subscription_id) = eose_rx.recv().await {
                    if subscription_id == all_messages {
                        _ = async_cx.update_global::<ChatRegistry, _>(|state, cx| {
                            state.load(cx);
                        });
                    }
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
