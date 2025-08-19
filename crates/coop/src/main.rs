use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Error};
use assets::Assets;
use common::event::EventUtils;
use global::constants::{
    APP_ID, APP_NAME, BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT, METADATA_BATCH_TIMEOUT,
    SEARCH_RELAYS, WAIT_FOR_FINISH,
};
use global::{nostr_client, processed_events, starting_time, NostrSignal};
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

    // Initialize the starting time
    let _ = starting_time();

    // Initialize the Application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    let (signal_tx, signal_rx) = channel::bounded::<NostrSignal>(2048);
    let (mta_tx, mta_rx) = channel::bounded::<PublicKey>(1024);
    let (event_tx, event_rx) = channel::bounded::<Event>(2048);
    let signal_tx_clone = signal_tx.clone();

    app.background_executor()
        .spawn(async move {
            // Subscribe for app updates from the bootstrap relays.
            if let Err(e) = connect(client).await {
                log::error!("Failed to connect to bootstrap relays: {e}");
            }

            // Handle Nostr notifications.
            //
            // Send the redefined signal back to GPUI via channel.
            if let Err(e) = handle_nostr_notifications(&signal_tx_clone, &event_tx).await {
                log::error!("Failed to handle Nostr notifications: {e}");
            }
        })
        .detach();

    app.background_executor()
        .spawn(async move {
            let duration = Duration::from_millis(METADATA_BATCH_TIMEOUT);
            let mut processed_pubkeys: BTreeSet<PublicKey> = BTreeSet::new();
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
                        // Prevent duplicate keys from being processed
                        if processed_pubkeys.insert(public_key) {
                            batch.insert(public_key);
                        }
                        // Process the batch if it's full
                        if batch.len() >= METADATA_BATCH_LIMIT {
                            sync_data_for_pubkeys(std::mem::take(&mut batch)).await;
                        }
                    }
                    BatchEvent::Timeout => {
                        if !batch.is_empty() {
                            sync_data_for_pubkeys(std::mem::take(&mut batch)).await;
                        }
                    }
                    BatchEvent::Closed => {
                        if !batch.is_empty() {
                            sync_data_for_pubkeys(std::mem::take(&mut batch)).await;
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
                    smol::Timer::after(Duration::from_secs(1)).await;
                    continue;
                }

                let duration = smol::Timer::after(Duration::from_secs(WAIT_FOR_FINISH));

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
                        let cached = unwrap_gift(&event, &signal_tx, &mta_tx).await;

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
                    }
                }
            }
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
                    while let Ok(signal) = signal_rx.recv().await {
                        cx.update(|window, cx| {
                            let registry = Registry::global(cx);
                            let identity = Identity::global(cx);

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
                                // Add the new metadata to the registry or update the existing one
                                NostrSignal::Metadata(event) => {
                                    registry.update(cx, |this, cx| {
                                        this.insert_or_update_person(event, cx);
                                    });
                                }
                                // Convert the gift wrapped message to a message
                                NostrSignal::GiftWrap(event) => {
                                    if let Some(public_key) = identity.read(cx).public_key() {
                                        registry.update(cx, |this, cx| {
                                            this.event_to_message(public_key, event, window, cx);
                                        });
                                    }
                                }
                                NostrSignal::DmRelaysFound => {
                                    identity.update(cx, |this, cx| {
                                        this.set_has_dm_relays(cx);
                                    });
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
    signal_tx: &Sender<NostrSignal>,
    event_tx: &Sender<Event>,
) -> Result<(), Error> {
    let client = nostr_client();
    let auto_close = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
    let mut notifications = client.notifications();

    while let Ok(notification) = notifications.recv().await {
        let RelayPoolNotification::Message { message, .. } = notification else {
            continue;
        };

        let RelayMessage::Event { event, .. } = message else {
            continue;
        };

        // Skip events that have already been processed
        if !processed_events().write().await.insert(event.id) {
            continue;
        }

        match event.kind {
            Kind::RelayList => {
                // Get metadata for event's pubkey that matches the current user's pubkey
                if let Ok(true) = is_from_current_user(&event).await {
                    let sub_id = SubscriptionId::new("metadata");
                    let filter = Filter::new()
                        .kinds(vec![Kind::Metadata, Kind::ContactList, Kind::InboxRelays])
                        .author(event.pubkey)
                        .limit(10);

                    client
                        .subscribe_with_id(sub_id, filter, Some(auto_close))
                        .await
                        .ok();
                }
            }
            Kind::InboxRelays => {
                if let Ok(true) = is_from_current_user(&event).await {
                    // Get all inbox relays
                    let relays = event
                        .tags
                        .filter_standardized(TagKind::Relay)
                        .filter_map(|t| {
                            if let TagStandard::Relay(url) = t {
                                Some(url.to_owned())
                            } else {
                                None
                            }
                        })
                        .collect_vec();

                    if !relays.is_empty() {
                        // Add relays to nostr client
                        for relay in relays.iter() {
                            _ = client.add_relay(relay).await;
                            _ = client.connect_relay(relay).await;
                        }

                        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(event.pubkey);
                        let sub_id = SubscriptionId::new("gift-wrap");

                        // Notify the UI that the current user has set up the DM relays
                        signal_tx.send(NostrSignal::DmRelaysFound).await.ok();

                        if client
                            .subscribe_with_id_to(relays.clone(), sub_id, filter, None)
                            .await
                            .is_ok()
                        {
                            log::info!("Subscribing to messages in: {relays:?}");
                        }
                    }
                }
            }
            Kind::ContactList => {
                if let Ok(true) = is_from_current_user(&event).await {
                    let public_keys: Vec<PublicKey> = event.tags.public_keys().copied().collect();
                    let kinds = vec![Kind::Metadata, Kind::ContactList];
                    let lens = public_keys.len() * kinds.len();
                    let filter = Filter::new().limit(lens).authors(public_keys).kinds(kinds);

                    client
                        .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(auto_close))
                        .await
                        .ok();
                }
            }
            Kind::Metadata => {
                signal_tx
                    .send(NostrSignal::Metadata(event.into_owned()))
                    .await
                    .ok();
            }
            Kind::GiftWrap => {
                event_tx.send(event.into_owned()).await.ok();
            }
            _ => {}
        }
    }

    Ok(())
}

async fn is_from_current_user(event: &Event) -> Result<bool, Error> {
    let client = nostr_client();
    let signer = client.signer().await?;
    let public_key = signer.get_public_key().await?;

    Ok(public_key == event.pubkey)
}

async fn sync_data_for_pubkeys(public_keys: BTreeSet<PublicKey>) {
    let client = nostr_client();
    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
    let kinds = vec![Kind::Metadata, Kind::ContactList];

    let filter = Filter::new()
        .limit(public_keys.len() * kinds.len())
        .authors(public_keys)
        .kinds(kinds);

    client
        .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
        .await
        .ok();
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
async fn unwrap_gift(
    gift: &Event,
    signal_tx: &Sender<NostrSignal>,
    mta_tx: &Sender<PublicKey>,
) -> bool {
    let client = nostr_client();
    let mut is_cached = false;

    let event = match get_unwrapped(gift.id).await {
        Ok(event) => {
            is_cached = true;
            event
        }
        Err(_) => {
            match client.unwrap_gift_wrap(gift).await {
                Ok(unwrap) => {
                    // Sign the unwrapped event with a RANDOM KEYS
                    let Ok(unwrapped) = unwrap.rumor.sign_with_keys(&Keys::generate()) else {
                        log::error!("Failed to sign event");
                        return false;
                    };

                    // Save this event to the database for future use.
                    if let Err(e) = set_unwrapped(gift.id, &unwrapped).await {
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

    // Send all pubkeys to the metadata batch to sync data
    for public_key in event.all_pubkeys() {
        mta_tx.send(public_key).await.ok();
    }

    // Send a notify to GPUI if this is a new message
    if starting_time() <= &event.created_at {
        signal_tx.send(NostrSignal::GiftWrap(event)).await.ok();
    }

    is_cached
}
