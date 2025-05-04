use anyhow::{anyhow, Error};
use asset::Assets;
use auto_update::AutoUpdater;
use chats::ChatRegistry;
use futures::{select, FutureExt};
#[cfg(not(target_os = "linux"))]
use global::constants::APP_NAME;
use global::{
    constants::{
        ALL_MESSAGES_SUB_ID, APP_ID, APP_PUBKEY, BOOTSTRAP_RELAYS, NEW_MESSAGE_SUB_ID,
        SEARCH_RELAYS,
    },
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
    nips::nip01::Coordinate, pool::prelude::ReqExitPolicy, Client, Event, EventBuilder, EventId,
    Filter, JsonUtil, Keys, Kind, Metadata, PublicKey, RelayMessage, RelayPoolNotification,
    SubscribeAutoCloseOptions, SubscriptionId, Tag,
};
use smol::Timer;
use std::{collections::HashSet, mem, sync::Arc, time::Duration};
use ui::{theme::Theme, Root};

pub(crate) mod asset;
pub(crate) mod chatspace;
pub(crate) mod lru_cache;
pub(crate) mod views;

actions!(coop, [Quit]);

#[derive(Debug)]
enum Signal {
    /// Receive event
    Event(Event),
    /// Receive metadata
    Metadata(Box<(PublicKey, Option<Metadata>)>),
    /// Receive eose
    Eose,
    /// Receive app updates
    AppUpdates(Event),
}

fn main() {
    // Enable logging
    tracing_subscriber::fmt::init();
    // Fix crash on startup
    _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let (event_tx, event_rx) = smol::channel::bounded::<Signal>(2048);
    let (batch_tx, batch_rx) = smol::channel::bounded::<Vec<PublicKey>>(500);

    // Initialize nostr client
    let client = get_client();
    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

    // Initialize application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    // Connect to default relays
    app.background_executor()
        .spawn(async move {
            for relay in BOOTSTRAP_RELAYS.into_iter() {
                if let Err(e) = client.add_relay(relay).await {
                    log::error!("Failed to add relay {}: {}", relay, e);
                }
            }

            for relay in SEARCH_RELAYS.into_iter() {
                if let Err(e) = client.add_relay(relay).await {
                    log::error!("Failed to add relay {}: {}", relay, e);
                }
            }

            // Establish connection to bootstrap relays
            client.connect().await;

            log::info!("Connected to bootstrap relays");
            log::info!("Subscribing to app updates...");

            let coordinate = Coordinate {
                kind: Kind::Custom(32267),
                public_key: PublicKey::from_hex(APP_PUBKEY).expect("App Pubkey is invalid"),
                identifier: APP_ID.into(),
            };

            let filter = Filter::new()
                .kind(Kind::ReleaseArtifactSet)
                .coordinate(&coordinate)
                .limit(1);

            if let Err(e) = client
                .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                .await
            {
                log::error!("Failed to subscribe for app updates: {}", e);
            }
        })
        .detach();

    // Handle batch metadata
    app.background_executor()
        .spawn(async move {
            const BATCH_SIZE: usize = 500;
            const BATCH_TIMEOUT: Duration = Duration::from_millis(300);

            let mut batch: HashSet<PublicKey> = HashSet::new();

            loop {
                let mut timeout = Box::pin(Timer::after(BATCH_TIMEOUT).fuse());

                select! {
                    pubkeys = batch_rx.recv().fuse() => {
                        match pubkeys {
                            Ok(keys) => {
                                batch.extend(keys);
                                if batch.len() >= BATCH_SIZE {
                                    sync_metadata(mem::take(&mut batch), client, opts).await;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    _ = timeout => {
                        if !batch.is_empty() {
                            sync_metadata(mem::take(&mut batch), client, opts).await;
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
                                    let event = match get_unwrapped(event.id).await {
                                        Ok(event) => event,
                                        Err(_) => match client.unwrap_gift_wrap(&event).await {
                                            Ok(unwrap) => {
                                                match unwrap.rumor.sign_with_keys(&rng_keys) {
                                                    Ok(ev) => {
                                                        set_unwrapped(event.id, &ev, &rng_keys)
                                                            .await
                                                            .ok();
                                                        ev
                                                    }
                                                    Err(_) => continue,
                                                }
                                            }
                                            Err(_) => continue,
                                        },
                                    };

                                    let mut pubkeys = vec![];
                                    pubkeys.extend(event.tags.public_keys());
                                    pubkeys.push(event.pubkey);

                                    // Send all pubkeys to the batch to sync metadata
                                    batch_tx.send(pubkeys).await.ok();

                                    // Save the event to the database, use for query directly.
                                    client.database().save_event(&event).await.ok();

                                    // Send this event to the GPUI
                                    if new_id == *subscription_id {
                                        event_tx.send(Signal::Event(event)).await.ok();
                                    }
                                }
                                Kind::Metadata => {
                                    let metadata = Metadata::from_json(&event.content).ok();

                                    event_tx
                                        .send(Signal::Metadata(Box::new((event.pubkey, metadata))))
                                        .await
                                        .ok();
                                }
                                Kind::ContactList => {
                                    if let Ok(signer) = client.signer().await {
                                        if let Ok(public_key) = signer.get_public_key().await {
                                            if public_key == event.pubkey {
                                                let pubkeys = event
                                                    .tags
                                                    .public_keys()
                                                    .copied()
                                                    .collect::<Vec<_>>();

                                                batch_tx.send(pubkeys).await.ok();
                                            }
                                        }
                                    }
                                }
                                Kind::ReleaseArtifactSet => {
                                    let filter = Filter::new()
                                        .ids(event.tags.event_ids().copied())
                                        .kind(Kind::FileMetadata);

                                    if let Err(e) = client
                                        .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                                        .await
                                    {
                                        log::error!("Failed to subscribe for file metadata: {}", e);
                                    } else {
                                        event_tx
                                            .send(Signal::AppUpdates(event.into_owned()))
                                            .await
                                            .ok();
                                    }
                                }
                                _ => {}
                            }
                        }
                        RelayMessage::EndOfStoredEvents(subscription_id) => {
                            if all_id == *subscription_id {
                                event_tx.send(Signal::Eose).await.ok();
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
            #[cfg(not(target_os = "linux"))]
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            // Root Entity
            cx.new(|cx| {
                // Initialize components
                ui::init(cx);

                // Initialize auto update
                auto_update::init(cx);

                // Initialize chat state
                chats::init(cx);

                // Initialize account state
                account::init(cx);

                // Spawn a task to handle events from nostr channel
                cx.spawn_in(window, async move |_, cx| {
                    while let Ok(signal) = event_rx.recv().await {
                        cx.update(|window, cx| {
                            let chats = ChatRegistry::global(cx);
                            let auto_updater = AutoUpdater::global(cx);

                            match signal {
                                Signal::Event(event) => {
                                    chats.update(cx, |this, cx| {
                                        this.push_message(event, window, cx)
                                    });
                                }
                                Signal::Metadata(data) => {
                                    chats.update(cx, |this, cx| {
                                        this.add_profile(data.0, data.1, cx)
                                    });
                                }
                                Signal::Eose => {
                                    chats.update(cx, |this, cx| {
                                        // This function maybe called multiple times
                                        // TODO: only handle the last EOSE signal
                                        this.load_rooms(window, cx)
                                    });
                                }
                                Signal::AppUpdates(event) => {
                                    // TODO: add settings for auto updates
                                    auto_updater.update(cx, |this, cx| {
                                        this.update(event, cx);
                                    })
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

async fn set_unwrapped(root: EventId, event: &Event, keys: &Keys) -> Result<(), Error> {
    let client = get_client();
    let event = EventBuilder::new(Kind::Custom(9001), event.as_json())
        .tags(vec![Tag::event(root)])
        .sign(keys)
        .await?;

    client.database().save_event(&event).await?;

    Ok(())
}

async fn get_unwrapped(gift_wrap: EventId) -> Result<Event, Error> {
    let client = get_client();
    let filter = Filter::new()
        .kind(Kind::Custom(9001))
        .event(gift_wrap)
        .limit(1);

    if let Some(event) = client.database().query(filter).await?.first_owned() {
        let parsed = Event::from_json(event.content)?;
        Ok(parsed)
    } else {
        Err(anyhow!("Event not found"))
    }
}

async fn sync_metadata(
    buffer: HashSet<PublicKey>,
    client: &Client,
    opts: SubscribeAutoCloseOptions,
) {
    let kinds = vec![
        Kind::Metadata,
        Kind::ContactList,
        Kind::InboxRelays,
        Kind::UserStatus,
    ];

    let filter = Filter::new()
        .authors(buffer.iter().cloned())
        .limit(buffer.len() * kinds.len())
        .kinds(kinds);

    if let Err(e) = client
        .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
        .await
    {
        log::error!("Failed to sync metadata: {e}");
    }
}

fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
