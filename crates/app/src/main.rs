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
use tokio::{
    sync::{mpsc, Mutex},
    time::sleep,
};

use constants::{ALL_MESSAGES_SUB_ID, APP_NAME, FAKE_SIG, METADATA_DELAY, NEW_MESSAGE_SUB_ID};
use states::{
    account::AccountRegistry,
    chat::ChatRegistry,
    metadata::MetadataRegistry,
    signal::{Signal, SignalRegistry},
};
use views::app::AppView;

pub mod asset;
pub mod constants;
pub mod states;
pub mod utils;
pub mod views;

actions!(main_menu, [Quit]);
actions!(app, [ReloadMetadata]);

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

    // Signal
    let (signal_tx, mut signal_rx) = mpsc::channel::<Signal>(10000);
    let (mta_tx, mut mta_rx) = mpsc::unbounded_channel::<PublicKey>();

    // Re use sender
    let mta_tx_clone = mta_tx.clone();

    // Handle notification from Relays
    // Send notfiy back to GPUI
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
                        match client.unwrap_gift_wrap(&event).await {
                            Ok(UnwrappedGift { rumor, .. }) => {
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
                                        if let Err(e) = signal_tx.send(Signal::RecvEvent(ev)).await
                                        {
                                            println!("Error: {}", e)
                                        }
                                    }
                                }
                            }
                            Err(e) => println!("Error: {}", e),
                        }
                    } else if event.kind == Kind::Metadata {
                        if let Err(e) = signal_tx.send(Signal::RecvMetadata(event.pubkey)).await {
                            println!("Error: {}", e)
                        }
                    }
                } else if let RelayMessage::EndOfStoredEvents(subscription_id) = message {
                    if let Err(e) = signal_tx.send(Signal::RecvEose(subscription_id)).await {
                        println!("Error: {}", e)
                    }
                }
            }
        }
    });

    // Handle metadata request
    // Merge all requests into single subscription
    tokio::spawn(async move {
        let queue: Arc<Mutex<HashSet<PublicKey>>> = Arc::new(Mutex::new(HashSet::new()));
        let queue_clone = queue.clone();

        let (tx, mut rx) = mpsc::channel::<PublicKey>(200);

        tokio::spawn(async move {
            while let Some(public_key) = mta_rx.recv().await {
                queue_clone.lock().await.insert(public_key);
                _ = tx.send(public_key).await;
            }
        });

        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                sleep(Duration::from_millis(METADATA_DELAY)).await;

                let authors: Vec<PublicKey> = queue.lock().await.drain().collect();
                let total = authors.len();

                if total > 0 {
                    let filter = Filter::new()
                        .authors(authors)
                        .kind(Kind::Metadata)
                        .limit(total);
                    let opts = SubscribeAutoCloseOptions::default()
                        .filter(FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(2)));

                    if let Err(e) = client.subscribe(vec![filter], Some(opts)).await {
                        println!("Error: {}", e);
                    }
                }
            }
        });
    });

    App::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()))
        .run(move |cx| {
            // Account state
            AccountRegistry::set_global(cx);
            // Metadata state
            MetadataRegistry::set_global(cx);
            // Chat state
            ChatRegistry::set_global(cx);
            // Signal state
            SignalRegistry::set_global(cx, mta_tx_clone);

            // Initialize components
            coop_ui::init(cx);

            // Set quit action
            cx.on_action(quit);

            /*
            cx.spawn(|async_cx| async move {
                let accounts = get_all_accounts_from_keyring();

                // Automatically Login if only habe 1 account
                if let Some(account) = accounts.into_iter().next() {
                    if let Ok(keys) = get_keys_by_account(account) {
                        get_client().set_signer(keys).await;

                        _ = async_cx.update_global::<AccountRegistry, _>(|state, _| {
                            state.set_user(Some(account));
                        });
                    }
                }
            })
            .detach();
            */

            cx.spawn(|async_cx| async move {
                let all_messages = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
                let mut is_initialized = false;

                while let Some(signal) = signal_rx.recv().await {
                    match signal {
                        Signal::RecvEose(id) => {
                            if id == all_messages {
                                if !is_initialized {
                                    _ = async_cx.update_global::<ChatRegistry, _>(|state, _| {
                                        state.set_init();
                                    });

                                    is_initialized = true;
                                } else {
                                    _ = async_cx.update_global::<ChatRegistry, _>(|state, _| {
                                        state.set_reload();
                                    });
                                }
                            }
                        }
                        Signal::RecvMetadata(public_key) => {
                            _ = async_cx.update_global::<MetadataRegistry, _>(|state, _cx| {
                                state.seen(public_key);
                            })
                        }
                        Signal::RecvEvent(event) => {
                            _ = async_cx.update_global::<ChatRegistry, _>(|state, _| {
                                state.push(event);
                            });
                        }
                        _ => {}
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