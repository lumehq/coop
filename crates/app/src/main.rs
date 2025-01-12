use asset::Assets;
use constants::{
    ALL_MESSAGES_SUB_ID, APP_NAME, FAKE_SIG, KEYRING_SERVICE, METADATA_DELAY, NEW_MESSAGE_SUB_ID,
};
use dirs::config_dir;
use gpui::{
    actions, point, px, size, App, AppContext, Bounds, SharedString, TitlebarOptions,
    VisualContext, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};
use nostr_sdk::prelude::*;
use states::{app::AppRegistry, chat::ChatRegistry};
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
use ui::Root;
use views::app::AppView;

mod asset;
mod constants;
mod states;
mod utils;
mod views;

actions!(main_menu, [Quit]);
actions!(app, [ReloadMetadata]);

static CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Clone)]
pub enum Signal {
    /// Receive event
    Event(Event),
    /// Receive metadata
    Metadata(PublicKey),
    /// Receive EOSE
    Eose,
}

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
    let (signal_tx, mut signal_rx) = mpsc::channel::<Signal>(4096);
    let (mta_tx, mut mta_rx) = mpsc::channel::<PublicKey>(4096);

    // Handle notification from Relays
    // Send notify back to GPUI
    tokio::spawn(async move {
        let sig = Signature::from_str(FAKE_SIG).unwrap();
        let new_message = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
        let all_messages = SubscriptionId::new(ALL_MESSAGES_SUB_ID);

        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Message { message, .. } = notification {
                if let RelayMessage::Event {
                    event,
                    subscription_id,
                } = message
                {
                    match event.kind {
                        Kind::GiftWrap => {
                            match client.unwrap_gift_wrap(&event).await {
                                Ok(UnwrappedGift { mut rumor, sender }) => {
                                    // Request metadata
                                    if let Err(e) = mta_tx.send(sender).await {
                                        println!("Send error: {}", e)
                                    };

                                    // Compute event id if not exist
                                    rumor.ensure_id();

                                    if let Some(id) = rumor.id {
                                        let ev = Event::new(
                                            id,
                                            rumor.pubkey,
                                            rumor.created_at,
                                            rumor.kind,
                                            rumor.tags,
                                            rumor.content,
                                            sig,
                                        );

                                        // Save rumor to database to further query
                                        if let Err(e) = client.database().save_event(&ev).await {
                                            println!("Save error: {}", e);
                                        }

                                        // Send event back to channel
                                        if subscription_id == new_message {
                                            if let Err(e) = signal_tx.send(Signal::Event(ev)).await
                                            {
                                                println!("Send error: {}", e)
                                            }
                                        }
                                    }
                                }
                                Err(e) => println!("Unwrap error: {}", e),
                            }
                        }
                        Kind::ContactList => {
                            let public_keys: Vec<PublicKey> =
                                event.tags.public_keys().copied().collect();

                            for public_key in public_keys.into_iter() {
                                if let Err(e) = mta_tx.send(public_key).await {
                                    println!("Send error: {}", e)
                                };
                            }
                        }
                        Kind::Metadata => {
                            if let Err(e) = signal_tx.send(Signal::Metadata(event.pubkey)).await {
                                println!("Send error: {}", e)
                            }
                        }
                        _ => {}
                    }
                } else if let RelayMessage::EndOfStoredEvents(subscription_id) = message {
                    if subscription_id == all_messages {
                        if let Err(e) = signal_tx.send(Signal::Eose).await {
                            println!("Send error: {}", e)
                        }
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

                    if let Err(e) = client.sync(filter, &SyncOptions::default()).await {
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
            // App state
            AppRegistry::set_global(cx);
            // Chat state
            ChatRegistry::set_global(cx);

            // Initialize components
            ui::init(cx);

            // Set quit action
            cx.on_action(quit);

            cx.spawn(|async_cx| {
                let task = cx.read_credentials(KEYRING_SERVICE);

                async move {
                    if let Ok(res) = task.await {
                        if let Some((npub, secret)) = res {
                            let public_key = PublicKey::from_bech32(&npub).unwrap();
                            let hex = String::from_utf8(secret).unwrap();
                            let keys = Keys::parse(&hex).unwrap();

                            // Update signer
                            async_cx
                                .background_executor()
                                .spawn(async move { client.set_signer(keys).await })
                                .detach();

                            // Update global state
                            _ = async_cx.update_global::<AppRegistry, _>(|state, cx| {
                                state.set_user(public_key, cx);
                            });
                        } else {
                            _ = async_cx.update_global::<AppRegistry, _>(|state, _| {
                                state.set_loading();
                            });
                        }
                    } else {
                        _ = async_cx.update_global::<AppRegistry, _>(|state, _| {
                            state.set_loading();
                        });
                    }
                }
            })
            .detach();

            cx.spawn(|async_cx| async move {
                let (tx, rx) = smol::channel::unbounded::<Signal>();

                async_cx
                    .background_executor()
                    .spawn(async move {
                        while let Some(signal) = signal_rx.recv().await {
                            if let Err(e) = tx.send(signal).await {
                                println!("Send error: {}", e)
                            }
                        }
                    })
                    .detach();

                while let Ok(signal) = rx.recv().await {
                    match signal {
                        Signal::Eose => {
                            _ = async_cx.update_global::<ChatRegistry, _>(|chat, cx| {
                                chat.init(cx);
                            });
                        }
                        Signal::Metadata(public_key) => {
                            _ = async_cx.update_global::<AppRegistry, _>(|state, cx| {
                                state.set_refresh(public_key, cx);
                            });
                        }
                        Signal::Event(event) => {
                            _ = async_cx.update_global::<ChatRegistry, _>(|state, cx| {
                                state.receive(event, cx)
                            });
                        }
                    }
                }
            })
            .detach();

            // Set window size
            let bounds = Bounds::centered(None, size(px(900.0), px(680.0)), cx);

            let opts = WindowOptions {
                #[cfg(not(target_os = "linux"))]
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::new_static(APP_NAME)),
                    traffic_light_position: Some(point(px(9.0), px(9.0))),
                    appears_transparent: true,
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_decorations: Some(WindowDecorations::Client),
                #[cfg(target_os = "linux")]
                window_background: WindowBackgroundAppearance::Transparent,
                #[cfg(target_os = "linux")]
                window_decorations: Some(WindowDecorations::Client),
                kind: WindowKind::Normal,
                ..Default::default()
            };

            cx.open_window(opts, |cx| {
                let app_view = cx.new_view(AppView::new);

                cx.set_window_title("Coop");
                cx.activate(true);
                cx.new_view(|cx| Root::new(app_view.into(), cx))
            })
            .expect("System error");
        });
}

fn quit(_: &Quit, cx: &mut AppContext) {
    cx.quit();
}
