use asset::Assets;
use async_utility::task::spawn;
use chats::registry::ChatRegistry;
use common::{
    constants::{
        ALL_MESSAGES_SUB_ID, APP_ID, APP_NAME, FAKE_SIG, KEYRING_SERVICE, NEW_MESSAGE_SUB_ID,
    },
    profile::NostrProfile,
};
use gpui::{
    actions, px, size, App, AppContext, Application, AsyncApp, Bounds, KeyBinding, Menu, MenuItem,
    WindowBounds, WindowKind, WindowOptions,
};
#[cfg(not(target_os = "linux"))]
use gpui::{point, SharedString, TitlebarOptions};
#[cfg(target_os = "linux")]
use gpui::{WindowBackgroundAppearance, WindowDecorations};
use log::{error, info};
use nostr_sdk::prelude::*;
use state::{get_client, initialize_client};
use std::{borrow::Cow, collections::HashSet, str::FromStr, sync::Arc, time::Duration};
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
    // Issue: https://github.com/snapview/tokio-tungstenite/issues/353
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Initialize Nostr client
    initialize_client();

    // Get client
    let client = get_client();
    let (signal_tx, mut signal_rx) = tokio::sync::mpsc::channel::<Signal>(2048);

    spawn(async move {
        // Add some bootstrap relays
        _ = client.add_relay("wss://relay.damus.io/").await;
        _ = client.add_relay("wss://relay.primal.net/").await;
        _ = client.add_relay("wss://user.kindpag.es/").await;
        _ = client.add_relay("wss://directory.yabu.me/").await;
        _ = client.add_discovery_relay("wss://relaydiscovery.com").await;

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
            let mut buffer: HashSet<PublicKey> = HashSet::with_capacity(20);

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

    let app = Application::new()
        .with_assets(Assets)
        .with_http_client(Arc::new(reqwest_client::ReqwestClient::new()));

    app.on_reopen(move |cx| {
        let client = get_client();
        let (tx, rx) = oneshot::channel::<Option<NostrProfile>>();

        cx.spawn(|mut cx| async move {
            cx.background_spawn(async move {
                if let Ok(signer) = client.signer().await {
                    if let Ok(public_key) = signer.get_public_key().await {
                        let metadata = if let Ok(Some(metadata)) =
                            client.database().metadata(public_key).await
                        {
                            metadata
                        } else {
                            Metadata::new()
                        };

                        _ = tx.send(Some(NostrProfile::new(public_key, metadata)));
                    } else {
                        _ = tx.send(None);
                    }
                } else {
                    _ = tx.send(None);
                }
            })
            .detach();

            if let Ok(result) = rx.await {
                _ = restore_window(result, &mut cx).await;
            }
        })
        .detach();
    });

    app.run(move |cx| {
        // Initialize chat global state
        chats::registry::init(cx);
        // Initialize components
        ui::init(cx);
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

        cx.open_window(opts, |window, cx| {
            window.set_window_title(APP_NAME);
            window.set_app_id(APP_ID);
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            let handle = window.window_handle();
            let root = cx.new(|cx| Root::new(startup::init(window, cx).into(), window, cx));

            let task = cx.read_credentials(KEYRING_SERVICE);
            let (tx, rx) = oneshot::channel::<Option<NostrProfile>>();

            // Read credential in OS Keyring
            cx.background_spawn(async {
                let profile = if let Ok(Some((npub, secret))) = task.await {
                    let public_key = PublicKey::from_bech32(&npub).unwrap();
                    let secret_hex = String::from_utf8(secret).unwrap();
                    let keys = Keys::parse(&secret_hex).unwrap();

                    // Update nostr signer
                    _ = client.set_signer(keys).await;

                    // Get user's metadata
                    let metadata =
                        if let Ok(Some(metadata)) = client.database().metadata(public_key).await {
                            metadata
                        } else {
                            Metadata::new()
                        };

                    Some(NostrProfile::new(public_key, metadata))
                } else {
                    None
                };

                _ = tx.send(profile)
            })
            .detach();

            // Set root view based on credential status
            cx.spawn(|mut cx| async move {
                if let Ok(Some(profile)) = rx.await {
                    _ = cx.update_window(handle, |_, window, cx| {
                        window.replace_root(cx, |window, cx| {
                            Root::new(app::init(profile, window, cx).into(), window, cx)
                        });
                    });
                } else {
                    _ = cx.update_window(handle, |_, window, cx| {
                        window.replace_root(cx, |window, cx| {
                            Root::new(onboarding::init(window, cx).into(), window, cx)
                        });
                    });
                }
            })
            .detach();

            // Listen for messages from the Nostr thread
            cx.spawn(|cx| async move {
                while let Some(signal) = signal_rx.recv().await {
                    match signal {
                        Signal::Eose => {
                            _ = cx.update(|cx| {
                                if let Some(chats) = ChatRegistry::global(cx) {
                                    chats.update(cx, |this, cx| this.load_chat_rooms(cx))
                                }
                            });
                        }
                        Signal::Event(event) => {
                            _ = cx.update(|cx| {
                                if let Some(chats) = ChatRegistry::global(cx) {
                                    chats.update(cx, |this, cx| this.push_message(event, cx))
                                }
                            });
                        }
                    }
                }
            })
            .detach();

            root
        })
        .expect("System error. Please re-open the app.");
    });
}

async fn restore_window(profile: Option<NostrProfile>, cx: &mut AsyncApp) -> Result<()> {
    let opts = cx
        .update(|cx| WindowOptions {
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
        })
        .expect("Failed to set window options.");

    if let Some(profile) = profile {
        _ = cx.open_window(opts, |window, cx| {
            window.set_window_title(APP_NAME);
            window.set_app_id(APP_ID);
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            cx.new(|cx| Root::new(app::init(profile, window, cx).into(), window, cx))
        });
    } else {
        _ = cx.open_window(opts, |window, cx| {
            window.set_window_title(APP_NAME);
            window.set_app_id(APP_ID);
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            cx.new(|cx| Root::new(onboarding::init(window, cx).into(), window, cx))
        });
    };

    Ok(())
}

fn quit(_: &Quit, cx: &mut App) {
    info!("Gracefully quitting the application . . .");
    cx.quit();
}
