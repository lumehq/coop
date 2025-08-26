use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Error};
use assets::Assets;
use common::event::EventUtils;
use global::constants::{
    APP_ID, APP_NAME, BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT, METADATA_BATCH_TIMEOUT,
    SEARCH_RELAYS, TOTAL_RETRY, WAIT_FOR_FINISH,
};
use global::{global_channel, nostr_client, processed_events, starting_time, NostrSignal};
use gpui::{
    actions, point, px, size, App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem,
    SharedString, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowKind, WindowOptions,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smol::channel::Sender;
use smol::lock::Mutex as AsyncMutex;
use theme::Theme;
use ui::Root;

pub(crate) mod chatspace;
pub(crate) mod views;

i18n::init!();

actions!(coop, [Quit]);

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
enum LocalRelayStatus {
    #[default]
    Waiting,
    NotFound,
    Found,
}

#[derive(Debug, Clone, Default)]
struct LocalRelayState {
    nip17: LocalRelayStatus,
    nip65: LocalRelayStatus,
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize the Nostr Client
    let client = nostr_client();

    // Initialize the starting time
    let _starting_time = starting_time();

    // Initialize the Application
    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    // Used for tracking NIP-65 and NIP-17 relay status
    let state = Arc::new(AsyncMutex::new(LocalRelayState::default()));
    let state_clone = state.clone();

    let (pubkey_tx, pubkey_rx) = smol::channel::bounded::<PublicKey>(1024);
    let (event_tx, event_rx) = smol::channel::bounded::<Event>(2048);

    app.background_executor()
        .spawn(async move {
            // Connect to bootstrap relays.
            if let Err(e) = connect(client).await {
                log::error!("Failed to connect to bootstrap relays: {e}");
            }

            // Handle Nostr notifications.
            //
            // Send the re-defined signal back to GPUI via the NostrSignal global channel.
            if let Err(e) = handle_nostr_notifications(&state, &event_tx).await {
                log::error!("Failed to handle Nostr notifications: {e}");
            }
        })
        .detach();

    app.background_executor()
        .spawn(async move {
            let channel = global_channel();
            let mut signer_set = false;
            let mut retry = 0;
            let mut nip65_retry = 0;

            loop {
                if signer_set {
                    let state = state_clone.lock().await;

                    if state.nip65 == LocalRelayStatus::Found {
                        if state.nip17 == LocalRelayStatus::Found {
                            break;
                        } else if state.nip17 == LocalRelayStatus::NotFound {
                            channel.0.send(NostrSignal::DmRelayNotFound).await.ok();
                            break;
                        } else {
                            retry += 1;
                            if retry == TOTAL_RETRY {
                                channel.0.send(NostrSignal::DmRelayNotFound).await.ok();
                                break;
                            }
                        }
                    } else {
                        nip65_retry += 1;
                        if nip65_retry == TOTAL_RETRY {
                            channel.0.send(NostrSignal::DmRelayNotFound).await.ok();
                            break;
                        }
                    }
                }

                if !signer_set {
                    if let Ok(signer) = client.signer().await {
                        if let Ok(public_key) = signer.get_public_key().await {
                            signer_set = true;

                            // Notify the app that the signer has been set.
                            channel
                                .0
                                .send(NostrSignal::SignerSet(public_key))
                                .await
                                .ok();

                            // Subscribe to the NIP-65 relays for the public key.
                            if let Err(e) = fetch_nip65_relays(public_key).await {
                                log::error!("Failed to fetch NIP-65 relays: {e}");
                            }
                        }
                    }
                }

                smol::Timer::after(Duration::from_secs(1)).await;
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
                    if let Ok(public_key) = pubkey_rx.recv().await {
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
            let channel = global_channel();
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
                        let cached = unwrap_gift(&event, &pubkey_tx).await;

                        // Increment the total messages counter if message is not from cache
                        if !cached {
                            counter += 1;
                        }

                        // Send partial finish signal to GPUI
                        if counter >= 20 {
                            channel.0.send(NostrSignal::PartialFinish).await.ok();
                            // Reset counter
                            counter = 0;
                        }
                    }
                    None => {
                        // Notify the UI that the processing is finished
                        channel.0.send(NostrSignal::Finish).await.ok();
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

                // Initialize client keys
                client_keys::init(cx);

                // Initialize app registry
                registry::init(cx);

                // Initialize settings
                settings::init(cx);

                // Initialize auto update
                auto_update::init(cx);

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
    state: &Arc<AsyncMutex<LocalRelayState>>,
    event_tx: &Sender<Event>,
) -> Result<(), Error> {
    let client = nostr_client();
    let channel = global_channel();
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
                    log::info!("Received relay list for the current user");

                    let mut state = state.lock().await;
                    state.nip65 = LocalRelayStatus::Found;

                    fetch_event(Kind::Metadata, event.pubkey).await;
                    fetch_event(Kind::ContactList, event.pubkey).await;
                    fetch_event(Kind::InboxRelays, event.pubkey).await;
                }
            }
            Kind::InboxRelays => {
                if let Ok(true) = is_from_current_user(&event).await {
                    log::info!("Received DM relays for the current user");

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
                        let mut state = state.lock().await;
                        state.nip17 = LocalRelayStatus::Found;

                        for relay in relays.iter() {
                            _ = client.add_relay(relay).await;
                            _ = client.connect_relay(relay).await;
                        }

                        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(event.pubkey);

                        if client
                            .subscribe_to(relays.clone(), filter, None)
                            .await
                            .is_ok()
                        {
                            log::info!("Subscribed to messages in: {relays:?}");
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
                channel
                    .0
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

async fn fetch_event(kind: Kind, public_key: PublicKey) {
    let client = nostr_client();
    let auto_close = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
    let filter = Filter::new().kind(kind).author(public_key).limit(1);

    if let Err(e) = client.subscribe(filter, Some(auto_close)).await {
        log::info!("Failed to subscribe: {e}");
    }
}

async fn fetch_nip65_relays(public_key: PublicKey) -> Result<(), Error> {
    let client = nostr_client();
    let auto_close = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

    let filter = Filter::new()
        .kind(Kind::RelayList)
        .author(public_key)
        .limit(1);

    client
        .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(auto_close))
        .await?;

    Ok(())
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

/// Checks if an event is belong to the current user
async fn is_from_current_user(event: &Event) -> Result<bool, Error> {
    let client = nostr_client();
    let signer = client.signer().await?;
    let public_key = signer.get_public_key().await?;

    Ok(public_key == event.pubkey)
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
async fn unwrap_gift(gift: &Event, pubkey_tx: &Sender<PublicKey>) -> bool {
    let client = nostr_client();
    let channel = global_channel();
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
        pubkey_tx.send(public_key).await.ok();
    }

    // Send a notify to GPUI if this is a new message
    if starting_time() <= &event.created_at {
        channel.0.send(NostrSignal::GiftWrap(event)).await.ok();
    }

    is_cached
}
