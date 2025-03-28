use asset::Assets;
use chats::ChatRegistry;
use futures::{select, FutureExt};
#[cfg(not(target_os = "linux"))]
use global::constants::APP_NAME;
use global::{
    constants::{ALL_MESSAGES_SUB_ID, APP_ID, BOOTSTRAP_RELAYS, NEW_MESSAGE_SUB_ID},
    get_client,
};
use gpui::{
    actions, px, size, App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem,
    WindowBounds, WindowKind, WindowOptions,
};
#[cfg(not(target_os = "linux"))]
use gpui::{point, SharedString, TitlebarOptions};
#[cfg(target_os = "linux")]
use gpui::{WindowBackgroundAppearance, WindowDecorations};
use nostr_sdk::{
    pool::prelude::ReqExitPolicy, Event, Filter, Keys, Kind, PublicKey, RelayMessage,
    RelayPoolNotification, SubscribeAutoCloseOptions, SubscriptionId,
};
use smol::Timer;
use std::{collections::HashSet, mem, sync::Arc, time::Duration};
use ui::{theme::Theme, Root};

pub(crate) mod asset;
pub(crate) mod chat_space;
pub(crate) mod views;

actions!(coop, [Quit]);

#[derive(Debug)]
enum Signal {
    /// Receive event
    Event(Event),
    /// Receive EOSE
    Eose,
}

fn main() {
    // Enable logging
    tracing_subscriber::fmt::init();
    // Fix crash on startup
    _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let (event_tx, event_rx) = smol::channel::bounded::<Signal>(1024);
    let (batch_tx, batch_rx) = smol::channel::bounded::<Vec<PublicKey>>(100);

    // Initialize nostr client
    let client = get_client();

    // Initialize application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    // Connect to default relays
    app.background_executor()
        .spawn(async {
            for relay in BOOTSTRAP_RELAYS.into_iter() {
                _ = client.add_relay(relay).await;
            }

            _ = client.add_discovery_relay("wss://relaydiscovery.com").await;
            _ = client.add_discovery_relay("wss://user.kindpag.es").await;

            _ = client.connect().await
        })
        .detach();

    // Handle batch metadata
    app.background_executor()
        .spawn(async move {
            const BATCH_SIZE: usize = 20;
            const BATCH_TIMEOUT: Duration = Duration::from_millis(500);

            let mut batch: HashSet<PublicKey> = HashSet::new();

            loop {
                let mut timeout = Box::pin(Timer::after(BATCH_TIMEOUT).fuse());

                select! {
                    pubkeys = batch_rx.recv().fuse() => {
                        match pubkeys {
                            Ok(keys) => {
                                batch.extend(keys);
                                if batch.len() >= BATCH_SIZE {
                                    handle_metadata(mem::take(&mut batch)).await;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    _ = timeout => {
                        if !batch.is_empty() {
                            handle_metadata(mem::take(&mut batch)).await;
                        }
                    }
                }
            }
        })
        .detach();

    // Handle notifications
    app.background_executor()
        .spawn(async move {
            let rng_keys = Keys::generate();
            let all_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
            let new_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
            let mut notifications = client.notifications();

            while let Ok(notification) = notifications.recv().await {
                if let RelayPoolNotification::Message { message, .. } = notification {
                    match message {
                        RelayMessage::Event {
                            event,
                            subscription_id,
                        } => {
                            match event.kind {
                                Kind::GiftWrap => {
                                    if let Ok(gift) = client.unwrap_gift_wrap(&event).await {
                                        // Sign the rumor with the generated keys,
                                        // this event will be used for internal only,
                                        // and NEVER send to relays.
                                        if let Ok(event) = gift.rumor.sign_with_keys(&rng_keys) {
                                            let mut pubkeys = vec![];
                                            pubkeys.extend(event.tags.public_keys());
                                            pubkeys.push(event.pubkey);

                                            // Save the event to the database, use for query directly.
                                            _ = client.database().save_event(&event).await;

                                            // Send this event to the GPUI
                                            if new_id == *subscription_id {
                                                _ = event_tx.send(Signal::Event(event)).await;
                                            }

                                            // Send all pubkeys to the batch
                                            _ = batch_tx.send(pubkeys).await;
                                        }
                                    }
                                }
                                Kind::ContactList => {
                                    let pubkeys =
                                        event.tags.public_keys().copied().collect::<HashSet<_>>();

                                    handle_metadata(pubkeys).await;
                                }
                                _ => {}
                            }
                        }
                        RelayMessage::EndOfStoredEvents(subscription_id) => {
                            if all_id == *subscription_id {
                                _ = event_tx.send(Signal::Eose).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
        })
        .detach();

    app.run(move |cx| {
        // Bring the app to the foreground
        cx.activate(true);

        // Register the `quit` function
        cx.on_action(quit);

        // Register the `quit` function with CMD+Q
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
                size(px(900.0), px(680.0)),
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
                // Initialize components
                ui::init(cx);
                // Initialize chat state
                chats::init(cx);
                // Initialize account state
                account::init(cx);
                // Spawn a task to handle events from nostr channel
                cx.spawn_in(window, async move |_, cx| {
                    let chats = cx.update(|_, cx| ChatRegistry::global(cx)).unwrap();

                    while let Ok(signal) = event_rx.recv().await {
                        cx.update(|window, cx| {
                            match signal {
                                Signal::Eose => {
                                    chats.update(cx, |this, cx| this.load_chat_rooms(window, cx));
                                }
                                Signal::Event(event) => {
                                    chats.update(cx, |this, cx| {
                                        this.push_message(event, window, cx)
                                    });
                                }
                            };
                        })
                        .ok();
                    }
                })
                .detach();

                Root::new(chat_space::init(window, cx).into(), window, cx)
            })
        })
        .expect("Failed to open window. Please restart the application.");
    });
}

async fn handle_metadata(buffer: HashSet<PublicKey>) {
    let client = get_client();

    let opts = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::ExitOnEOSE)
        .idle_timeout(Some(Duration::from_secs(1)));

    let filter = Filter::new()
        .authors(buffer.iter().cloned())
        .limit(100)
        .kinds(vec![Kind::Metadata, Kind::UserStatus]);

    if let Err(e) = client.subscribe(filter, Some(opts)).await {
        log::error!("Failed to sync metadata: {e}");
    }
}

fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
