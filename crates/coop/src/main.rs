use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Error};
use assets::Assets;
use common::event::EventUtils;
use global::constants::{
    ALL_MESSAGES_ID, APP_ID, APP_NAME, BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT,
    METADATA_BATCH_TIMEOUT, NEW_MESSAGE_ID, SEARCH_RELAYS,
};
use global::{nostr_client, NostrSignal};
use gpui::{
    actions, point, px, size, App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem,
    SharedString, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowKind, WindowOptions,
};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::Registry;
use smol::channel::{self, Sender};
use theme::Theme;
use ui::Root;

use crate::chatspace::ChatSpace;

pub(crate) mod chatspace;
pub(crate) mod views;

i18n::init!();

actions!(coop, [Quit]);

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize the Nostr Client
    let client = nostr_client();

    // Initialize the Application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    let (signal_tx, signal_rx) = channel::bounded::<NostrSignal>(2048);
    let (mta_tx, mta_rx) = channel::bounded::<PublicKey>(1024);
    let (event_tx, event_rx) = channel::unbounded::<Event>();

    let signal_tx_clone = signal_tx.clone();
    let mta_tx_clone = mta_tx.clone();

    app.background_executor()
        .spawn(async move {
            // Subscribe for app updates from the bootstrap relays.
            if let Err(e) = connect(client).await {
                log::error!("Failed to connect to bootstrap relays: {e}");
            }

            // Handle Nostr notifications.
            //
            // Send the redefined signal back to GPUI via channel.
            if let Err(e) =
                handle_nostr_notifications(client, &signal_tx_clone, &mta_tx_clone, &event_tx).await
            {
                log::error!("Failed to handle Nostr notifications: {e}");
            }
        })
        .detach();

    app.background_executor()
        .spawn(async move {
            let duration = Duration::from_millis(METADATA_BATCH_TIMEOUT);
            let mut batch: BTreeSet<PublicKey> = BTreeSet::new();

            /// Internal events for the metadata batching system
            enum BatchEvent {
                NewKeys(PublicKey),
                Timeout,
                Closed,
            }

            loop {
                let duration = smol::Timer::after(duration);

                let recv = || async {
                    if let Ok(public_key) = mta_rx.recv().await {
                        BatchEvent::NewKeys(public_key)
                    } else {
                        BatchEvent::Closed
                    }
                };

                let timeout = || async {
                    duration.await;
                    BatchEvent::Timeout
                };

                match smol::future::or(recv(), timeout()).await {
                    BatchEvent::NewKeys(public_key) => {
                        batch.insert(public_key);
                        // Process immediately if batch limit reached
                        if batch.len() >= METADATA_BATCH_LIMIT {
                            sync_data_for_pubkeys(client, std::mem::take(&mut batch)).await;
                        }
                    }
                    BatchEvent::Timeout => {
                        if !batch.is_empty() {
                            sync_data_for_pubkeys(client, std::mem::take(&mut batch)).await;
                        }
                    }
                    BatchEvent::Closed => {
                        if !batch.is_empty() {
                            sync_data_for_pubkeys(client, std::mem::take(&mut batch)).await;
                        }
                        break;
                    }
                }
            }
        })
        .detach();

    app.background_executor()
        .spawn(async move {
            let mut counter = 0;

            loop {
                // Signer is unset, probably user is not ready to retrieve gift wrap events
                if client.signer().await.is_err() {
                    continue;
                }

                let duration = smol::Timer::after(Duration::from_secs(30));

                let recv = || async {
                    // no inline
                    (event_rx.recv().await).ok()
                };

                let timeout = || async {
                    duration.await;
                    None
                };

                match smol::future::or(recv(), timeout()).await {
                    Some(event) => {
                        // Process the gift wrap event unwrapping
                        let cached = try_unwrap_event(&signal_tx, &mta_tx, &event, false).await;

                        // Increment the total messages counter if message is not from cache
                        if !cached {
                            counter += 1;
                        }

                        // Send partial finish signal to GPUI
                        if counter >= 20 {
                            signal_tx.send(NostrSignal::PartialFinish).await.ok();
                            // Reset counter
                            counter = 0;
                        }
                    }
                    None => {
                        // Notify the UI that the processing is finished
                        signal_tx.send(NostrSignal::Finish).await.ok();
                        // Mark all gift wraps as fetched
                        // For the next time Coop only needs to process new gift wraps
                        // set_all_gift_wraps_fetched().await;

                        break;
                    }
                }
            }

            // Event channel is no longer needed when all gift wrap events have been processed
            event_rx.close();
        })
        .detach();

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
                title: Some(SharedString::new_static(APP_NAME)),
                traffic_light_position: Some(point(px(9.0), px(9.0))),
                appears_transparent: true,
            }),
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
                // Initialize the tokio runtime
                gpui_tokio::init(cx);
                // Initialize components
                ui::init(cx);
                // Initialize app registry
                registry::init(cx);
                // Initialize settings
                settings::init(cx);
                // Initialize client keys
                client_keys::init(cx);
                // Initialize identity
                identity::init(window, cx);
                // Initialize auto update
                auto_update::init(cx);

                // Spawn a task to handle events from nostr channel
                cx.spawn_in(window, async move |_, cx| {
                    let all_messages = SubscriptionId::new(ALL_MESSAGES_ID);

                    while let Ok(signal) = signal_rx.recv().await {
                        cx.update(|window, cx| {
                            let registry = Registry::global(cx);
                            let identity = Identity::read_global(cx);

                            match signal {
                                // Load chat rooms and stop the loading status
                                NostrSignal::Finish => {
                                    registry.update(cx, |this, cx| {
                                        this.load_rooms(window, cx);
                                        this.set_loading(false, cx);
                                        // Send a signal to refresh all opened rooms' messages
                                        if let Some(ids) = ChatSpace::all_panels(window, cx) {
                                            this.refresh_rooms(ids, cx);
                                        }
                                    });
                                }
                                // Load chat rooms without setting as finished
                                NostrSignal::PartialFinish => {
                                    registry.update(cx, |this, cx| {
                                        this.load_rooms(window, cx);
                                        // Send a signal to refresh all opened rooms' messages
                                        if let Some(ids) = ChatSpace::all_panels(window, cx) {
                                            this.refresh_rooms(ids, cx);
                                        }
                                    });
                                }
                                // Load chat rooms without setting as finished
                                NostrSignal::Eose(subscription_id) => {
                                    // Only load chat rooms if the subscription ID matches the all_messages_sub_id
                                    if subscription_id == all_messages {
                                        registry.update(cx, |this, cx| {
                                            this.load_rooms(window, cx);
                                        });
                                    }
                                }
                                // Add the new metadata to the registry or update the existing one
                                NostrSignal::Metadata(event) => {
                                    registry.update(cx, |this, cx| {
                                        this.insert_or_update_person(event, cx);
                                    });
                                }
                                // Convert the gift wrapped message to a message
                                NostrSignal::GiftWrap(event) => {
                                    if let Some(public_key) = identity.public_key() {
                                        registry.update(cx, |this, cx| {
                                            this.event_to_message(public_key, event, window, cx);
                                        });
                                    }
                                }
                                NostrSignal::Notice(_msg) => {
                                    // window.push_notification(msg, cx);
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

fn load_embedded_fonts(cx: &App) {
    let asset_source = cx.asset_source();
    let font_paths = asset_source.list("fonts").unwrap();
    let embedded_fonts = Mutex::new(Vec::new());
    let executor = cx.background_executor();

    executor.block(executor.scoped(|scope| {
        for font_path in &font_paths {
            if !font_path.ends_with(".ttf") {
                continue;
            }

            scope.spawn(async {
                let font_bytes = asset_source.load(font_path).unwrap().unwrap();
                embedded_fonts.lock().unwrap().push(font_bytes);
            });
        }
    }));

    cx.text_system()
        .add_fonts(embedded_fonts.into_inner().unwrap())
        .unwrap();
}

fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}

async fn connect(client: &Client) -> Result<(), Error> {
    for relay in BOOTSTRAP_RELAYS.into_iter() {
        client.add_relay(relay).await?;
    }

    log::info!("Connected to bootstrap relays");

    for relay in SEARCH_RELAYS.into_iter() {
        client.add_relay(relay).await?;
    }

    log::info!("Connected to search relays");

    // Establish connection to relays
    client.connect().await;

    Ok(())
}

async fn handle_nostr_notifications(
    client: &Client,
    signal_tx: &Sender<NostrSignal>,
    mta_tx: &Sender<PublicKey>,
    event_tx: &Sender<Event>,
) -> Result<(), Error> {
    let new_messages = SubscriptionId::new(NEW_MESSAGE_ID);
    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

    let mut notifications = client.notifications();
    let mut processed_events: BTreeSet<EventId> = BTreeSet::new();
    let mut processed_dm_relays: BTreeSet<PublicKey> = BTreeSet::new();

    while let Ok(notification) = notifications.recv().await {
        let RelayPoolNotification::Message { message, .. } = notification else {
            continue;
        };

        match message {
            RelayMessage::Event {
                event,
                subscription_id,
            } => {
                if processed_events.contains(&event.id) {
                    continue;
                }
                // Skip events that have already been processed
                processed_events.insert(event.id);

                match event.kind {
                    Kind::GiftWrap => {
                        // Process to unwrap directly if event come from new messages subscription
                        // Otherwise, send the event to the event_tx channel
                        if *subscription_id == new_messages {
                            log::info!("receive a new message: {:?}", event.id);
                            try_unwrap_event(signal_tx, mta_tx, &event, true).await;
                        } else {
                            event_tx.send(event.into_owned()).await.ok();
                        }
                    }
                    Kind::Metadata => {
                        signal_tx
                            .send(NostrSignal::Metadata(event.into_owned()))
                            .await
                            .ok();
                    }
                    Kind::ContactList => {
                        if let Ok(true) = check_author(client, &event).await {
                            for public_key in event.tags.public_keys().copied() {
                                mta_tx.send(public_key).await.ok();
                            }
                        }
                    }
                    Kind::RelayList => {
                        if processed_dm_relays.contains(&event.pubkey) {
                            continue;
                        }
                        // Skip public keys that have already been processed
                        processed_dm_relays.insert(event.pubkey);

                        let filter = Filter::new()
                            .author(event.pubkey)
                            .kind(Kind::InboxRelays)
                            .limit(1);

                        if let Ok(output) = client.subscribe(filter, Some(opts)).await {
                            log::info!(
                                "Subscribed to get DM relays: {} - Relays: {:?}",
                                event.pubkey.to_bech32().unwrap(),
                                output.success
                            )
                        }
                    }
                    _ => {}
                }
            }
            RelayMessage::EndOfStoredEvents(subscription_id) => {
                signal_tx
                    .send(NostrSignal::Eose(subscription_id.into_owned()))
                    .await?;
            }
            _ => {}
        }
    }

    Ok(())
}

async fn check_author(client: &Client, event: &Event) -> Result<bool, Error> {
    let signer = client.signer().await?;
    let public_key = signer.get_public_key().await?;

    Ok(public_key == event.pubkey)
}

async fn sync_data_for_pubkeys(client: &Client, public_keys: BTreeSet<PublicKey>) {
    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
    let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];

    let filter = Filter::new()
        .limit(public_keys.len() * kinds.len())
        .authors(public_keys)
        .kinds(kinds);

    if let Err(e) = client
        .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
        .await
    {
        log::error!("Failed to sync metadata: {e}");
    }
}

/// Stores an unwrapped event in local database with reference to original
async fn set_unwrapped(root: EventId, unwrapped: &Event) -> Result<(), Error> {
    let client = nostr_client();

    // Save unwrapped event
    client.database().save_event(unwrapped).await?;

    // Create a reference event pointing to the unwrapped event
    let event = EventBuilder::new(Kind::ApplicationSpecificData, "")
        .tags(vec![Tag::identifier(root), Tag::event(unwrapped.id)])
        .sign(&Keys::generate())
        .await?;

    // Save reference event
    client.database().save_event(&event).await?;

    Ok(())
}

/// Retrieves a previously unwrapped event from local database
async fn get_unwrapped(root: EventId) -> Result<Event, Error> {
    let client = nostr_client();
    let filter = Filter::new()
        .kind(Kind::ApplicationSpecificData)
        .identifier(root)
        .limit(1);

    if let Some(event) = client.database().query(filter).await?.first_owned() {
        let target_id = event.tags.event_ids().collect_vec()[0];

        if let Some(event) = client.database().event_by_id(target_id).await? {
            Ok(event)
        } else {
            Err(anyhow!("Event not found."))
        }
    } else {
        Err(anyhow!("Event is not cached yet."))
    }
}

/// Unwraps a gift-wrapped event and processes its contents.
async fn try_unwrap_event(
    signal_tx: &Sender<NostrSignal>,
    mta_tx: &Sender<PublicKey>,
    event: &Event,
    incoming: bool,
) -> bool {
    let client = nostr_client();
    let mut is_cached = false;

    let event = match get_unwrapped(event.id).await {
        Ok(event) => {
            is_cached = true;
            event
        }
        Err(_) => {
            match client.unwrap_gift_wrap(event).await {
                Ok(unwrap) => {
                    // Sign the unwrapped event with a RANDOM KEYS
                    let Ok(unwrapped) = unwrap.rumor.sign_with_keys(&Keys::generate()) else {
                        log::error!("Failed to sign event");
                        return false;
                    };

                    // Save this event to the database for future use.
                    if let Err(e) = set_unwrapped(event.id, &unwrapped).await {
                        log::warn!("Failed to cache unwrapped event: {e}")
                    }

                    unwrapped
                }
                Err(e) => {
                    log::error!("Failed to unwrap event: {e}");
                    return false;
                }
            }
        }
    };

    // Get all pubkeys from the event
    let all_pubkeys = event.all_pubkeys();

    // Send all pubkeys to the metadata batch to sync data
    for public_key in all_pubkeys {
        mta_tx.send(public_key).await.ok();
    }

    // Send a notify to GPUI if this is a new message
    if incoming {
        signal_tx.send(NostrSignal::GiftWrap(event)).await.ok();
    }

    is_cached
}
