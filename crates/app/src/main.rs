use app_state::registry::AppRegistry;
use asset::Assets;
use async_utility::task::spawn;
use chat_state::registry::ChatRegistry;
use common::{
    constants::{
        ALL_MESSAGES_SUB_ID, APP_ID, APP_NAME, FAKE_SIG, KEYRING_SERVICE, NEW_MESSAGE_SUB_ID,
    },
    profile::NostrProfile,
};
use gpui::{
    actions, point, px, size, App, AppContext, Application, BorrowAppContext, Bounds, Menu,
    MenuItem, SharedString, TitlebarOptions, WindowBounds, WindowKind, WindowOptions,
};
#[cfg(target_os = "linux")]
use gpui::{WindowBackgroundAppearance, WindowDecorations};
use log::error;
use nostr_sdk::prelude::*;
use state::{get_client, initialize_client};
use std::{borrow::Cow, collections::HashSet, ops::Deref, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};
use ui::{theme::Theme, Root};
use views::{app, onboarding, startup};

mod asset;
mod views;

actions!(main_menu, [Quit]);

#[derive(Clone)]
pub enum Signal {
    /// Receive event
    Event(Event),
    /// Receive EOSE
    Eose,
}

fn main() {
    // Log
    tracing_subscriber::fmt::init();

    // Initialize nostr client
    initialize_client();

    // Get client
    let client = get_client();
    let (signal_tx, mut signal_rx) = tokio::sync::mpsc::channel::<Signal>(4096);

    spawn(async move {
        // Add some bootstrap relays
        _ = client.add_relay("wss://relay.damus.io/").await;
        _ = client.add_relay("wss://relay.primal.net/").await;
        _ = client.add_relay("wss://nos.lol/").await;

        _ = client.add_discovery_relay("wss://user.kindpag.es/").await;
        _ = client.add_discovery_relay("wss://directory.yabu.me/").await;

        // Connect to all relays
        _ = client.connect().await
    });

    spawn(async move {
        let (batch_tx, mut batch_rx) = mpsc::channel::<Cow<Event>>(20);

        async fn sync_metadata(client: &Client, buffer: &HashSet<PublicKey>) {
            let filter = Filter::new()
                .authors(buffer.iter().copied())
                .kind(Kind::Metadata)
                .limit(buffer.len());

            if let Err(e) = client.sync(filter, &SyncOptions::default()).await {
                error!("NEG error: {e}");
            }
        }

        async fn process_batch(client: &Client, events: &[Cow<'_, Event>]) {
            let sig = Signature::from_str(FAKE_SIG).unwrap();
            let mut buffer: HashSet<PublicKey> = HashSet::with_capacity(100);

            for event in events.iter() {
                if let Ok(UnwrappedGift { mut rumor, sender }) =
                    client.unwrap_gift_wrap(event).await
                {
                    let pubkeys: HashSet<PublicKey> = event.tags.public_keys().copied().collect();
                    buffer.extend(pubkeys);
                    buffer.insert(sender);

                    // Create event's ID is not exist
                    rumor.ensure_id();

                    // Save event to database
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

                        if let Err(e) = client.database().save_event(&ev).await {
                            error!("Save error: {}", e);
                        }
                    }
                }
            }

            sync_metadata(client, &buffer).await;
        }

        // Spawn a thread to handle batch process
        spawn(async move {
            const BATCH_SIZE: usize = 20;
            const BATCH_TIMEOUT: Duration = Duration::from_millis(200);

            let mut batch = Vec::with_capacity(20);
            let mut timeout = Box::pin(tokio::time::sleep(BATCH_TIMEOUT));

            loop {
                tokio::select! {
                    event = batch_rx.recv() => {
                        if let Some(event) = event {
                            batch.push(event);

                            if batch.len() == BATCH_SIZE {
                                process_batch(client, &batch).await;
                                batch.clear();
                                timeout = Box::pin(tokio::time::sleep(BATCH_TIMEOUT));
                            }
                        } else {
                            break;
                        }
                    }
                    _ = &mut timeout => {
                        if !batch.is_empty() {
                            process_batch(client, &batch).await;
                            batch.clear();
                        }
                        timeout = Box::pin(tokio::time::sleep(BATCH_TIMEOUT));
                    }
                }
            }
        });

        let all_id = SubscriptionId::new(ALL_MESSAGES_SUB_ID);
        let new_id = SubscriptionId::new(NEW_MESSAGE_SUB_ID);
        let sig = Signature::from_str(FAKE_SIG).unwrap();
        let mut notifications = client.notifications();

        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Message { message, .. } = notification {
                match message {
                    RelayMessage::Event {
                        event,
                        subscription_id,
                    } => match event.kind {
                        Kind::GiftWrap => {
                            if new_id == *subscription_id {
                                if let Ok(UnwrappedGift { mut rumor, .. }) =
                                    client.unwrap_gift_wrap(&event).await
                                {
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
                                            error!("Save error: {}", e);
                                        }

                                        // Send new event to GPUI
                                        if let Err(e) = signal_tx.send(Signal::Event(ev)).await {
                                            error!("Send error: {}", e)
                                        }
                                    }
                                }
                            }

                            if let Err(e) = batch_tx.send(event).await {
                                error!("Failed to add to batch: {}", e);
                            }
                        }
                        Kind::ContactList => {
                            let public_keys: HashSet<_> =
                                event.tags.public_keys().copied().collect();

                            sync_metadata(client, &public_keys).await;
                        }
                        _ => {}
                    },
                    RelayMessage::EndOfStoredEvents(subscription_id) => {
                        if all_id == *subscription_id {
                            if let Err(e) = signal_tx.send(Signal::Eose).await {
                                error!("Failed to send eose: {}", e)
                            };
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()))
        .run(move |cx| {
            // Initialize chat global state
            ChatRegistry::set_global(cx);

            // Initialize components
            ui::init(cx);

            cx.activate(true);
            cx.on_action(quit);
            cx.set_menus(vec![Menu {
                name: "Coop".into(),
                items: vec![MenuItem::action("Quit", Quit)],
            }]);

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
                ..Default::default()
            };

            let window = cx
                .open_window(opts, |window, cx| {
                    window.set_window_title(APP_NAME);
                    window.set_app_id(APP_ID);
                    window
                        .observe_window_appearance(|window, cx| {
                            Theme::sync_system_appearance(Some(window), cx);
                        })
                        .detach();

                    let root = cx.new(|cx| Root::new(startup::init(window, cx).into(), window, cx));
                    let weak_root = root.downgrade();
                    let window_handle = window.window_handle();
                    let task = cx.read_credentials(KEYRING_SERVICE);

                    // Initialize app global state
                    AppRegistry::set_global(weak_root, cx);

                    cx.spawn(|mut cx| async move {
                        if let Ok(Some((npub, secret))) = task.await {
                            let (tx, rx) = oneshot::channel::<NostrProfile>();

                            cx.background_executor()
                                .spawn(async move {
                                    let public_key = PublicKey::from_bech32(&npub).unwrap();
                                    let secret_hex = String::from_utf8(secret).unwrap();
                                    let keys = Keys::parse(&secret_hex).unwrap();

                                    // Update nostr signer
                                    _ = client.set_signer(keys).await;

                                    // Get user's metadata
                                    let metadata = if let Ok(Some(metadata)) =
                                        client.database().metadata(public_key).await
                                    {
                                        metadata
                                    } else {
                                        Metadata::new()
                                    };

                                    _ = tx.send(NostrProfile::new(public_key, metadata));
                                })
                                .detach();

                            if let Ok(profile) = rx.await {
                                cx.update_window(window_handle, |_, window, cx| {
                                    cx.update_global::<AppRegistry, _>(|this, cx| {
                                        this.set_user(Some(profile.clone()));
                                        this.set_root_view(
                                            app::init(profile, window, cx).into(),
                                            cx,
                                        );
                                    });
                                })
                                .unwrap();
                            }
                        } else {
                            cx.update_window(window_handle, |_, window, cx| {
                                cx.update_global::<AppRegistry, _>(|this, cx| {
                                    this.set_root_view(onboarding::init(window, cx).into(), cx);
                                });
                            })
                            .unwrap();
                        }
                    })
                    .detach();

                    root
                })
                .expect("System error. Please re-open the app.");

            // Listen for messages from the Nostr thread
            cx.spawn(|mut cx| async move {
                while let Some(signal) = signal_rx.recv().await {
                    match signal {
                        Signal::Eose => {
                            if let Err(e) =
                                cx.update_window(*window.deref(), |_this, window, cx| {
                                    cx.update_global::<ChatRegistry, _>(|this, cx| {
                                        this.load(window, cx);
                                    });
                                })
                            {
                                error!("Error: {}", e)
                            }
                        }
                        Signal::Event(event) => {
                            if let Err(e) =
                                cx.update_window(*window.deref(), |_this, window, cx| {
                                    cx.update_global::<ChatRegistry, _>(|this, cx| {
                                        this.new_room_message(event, window, cx);
                                    });
                                })
                            {
                                error!("Error: {}", e)
                            }
                        }
                    }
                }
            })
            .detach();
        });
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}
