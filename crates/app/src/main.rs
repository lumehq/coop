use asset::Assets;
use chats::registry::ChatRegistry;
use common::{
    constants::{ALL_MESSAGES_SUB_ID, APP_ID, APP_NAME, KEYRING_SERVICE, NEW_MESSAGE_SUB_ID},
    profile::NostrProfile,
};
use futures::{select, FutureExt};
use gpui::{
    actions, px, size, App, AppContext, Application, AsyncApp, Bounds, KeyBinding, Menu, MenuItem,
    WindowBounds, WindowKind, WindowOptions,
};
#[cfg(not(target_os = "linux"))]
use gpui::{point, SharedString, TitlebarOptions};
#[cfg(target_os = "linux")]
use gpui::{WindowBackgroundAppearance, WindowDecorations};
use log::{error, info};
use nostr_sdk::{
    pool::prelude::ReqExitPolicy, Client, Event, Filter, Keys, Kind, Metadata, PublicKey,
    RelayMessage, RelayPoolNotification, SubscribeAutoCloseOptions,
};
use nostr_sdk::{prelude::NostrEventsDatabaseExt, FromBech32, SubscriptionId};
use smol::Timer;
use state::get_client;
use std::{collections::HashSet, mem, sync::Arc, time::Duration};
use ui::{theme::Theme, Root};
use views::{app, onboarding, startup};

mod asset;
mod views;

actions!(coop, [Quit]);

#[derive(Clone)]
enum Signal {
    /// Receive event
    Event(Event),
    /// Receive EOSE
    Eose,
}

fn main() {
    // Fix crash on startup
    // TODO: why this is needed?
    _ = rustls::crypto::ring::default_provider().install_default();
    // Enable logging
    tracing_subscriber::fmt::init();

    let (event_tx, event_rx) = smol::channel::bounded::<Signal>(2048);
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
            _ = client.add_relay("wss://relay.damus.io/").await;
            _ = client.add_relay("wss://relay.primal.net/").await;
            _ = client.add_relay("wss://user.kindpag.es/").await;
            _ = client.add_relay("wss://purplepag.es/").await;
            _ = client.add_discovery_relay("wss://relaydiscovery.com").await;
            _ = client.connect().await
        })
        .detach();

    // Handle batch metadata
    app.background_executor()
        .spawn(async move {
            const BATCH_SIZE: usize = 20;
            const BATCH_TIMEOUT: Duration = Duration::from_millis(200);

            let mut batch: HashSet<PublicKey> = HashSet::new();

            loop {
                let mut timeout = Box::pin(Timer::after(BATCH_TIMEOUT).fuse());

                select! {
                    pubkeys = batch_rx.recv().fuse() => {
                        match pubkeys {
                            Ok(keys) => {
                                batch.extend(keys);
                                if batch.len() >= BATCH_SIZE {
                                    sync_metadata(client, mem::take(&mut batch)).await;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    _ = timeout => {
                        if !batch.is_empty() {
                            sync_metadata(client, mem::take(&mut batch)).await;
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
                                        let mut pubkeys = vec![];

                                        // Sign the rumor with the generated keys,
                                        // this event will be used for internal only,
                                        // and NEVER send to relays.
                                        if let Ok(event) = gift.rumor.sign_with_keys(&rng_keys) {
                                            pubkeys.extend(event.tags.public_keys());
                                            pubkeys.push(event.pubkey);

                                            // Save the event to the database, use for query directly.
                                            if let Err(e) =
                                                client.database().save_event(&event).await
                                            {
                                                error!("Failed to save event: {}", e);
                                            }

                                            // Send all pubkeys to the batch
                                            if let Err(e) = batch_tx.send(pubkeys).await {
                                                error!("Failed to send pubkeys to batch: {}", e)
                                            }

                                            // Send this event to the GPUI
                                            if new_id == *subscription_id {
                                                if let Err(e) =
                                                    event_tx.send(Signal::Event(event)).await
                                                {
                                                    error!("Failed to send event to GPUI: {}", e)
                                                }
                                            }
                                        }
                                    }
                                }
                                Kind::ContactList => {
                                    let pubkeys =
                                        event.tags.public_keys().copied().collect::<HashSet<_>>();
                                    sync_metadata(client, pubkeys).await;
                                }
                                _ => {}
                            }
                        }
                        RelayMessage::EndOfStoredEvents(subscription_id) => {
                            if all_id == *subscription_id {
                                if let Err(e) = event_tx.send(Signal::Eose).await {
                                    error!("Failed to send eose: {}", e)
                                };
                            }
                        }
                        _ => {}
                    }
                }
            }
        })
        .detach();

    // Handle re-open window
    app.on_reopen(move |cx| {
        let client = get_client();
        let (tx, rx) = oneshot::channel::<bool>();

        cx.background_spawn(async move {
            let is_login = client.signer().await.is_ok();
            _ = tx.send(is_login);
        })
        .detach();

        cx.spawn(|mut cx| async move {
            if let Ok(is_login) = rx.await {
                _ = restore_window(is_login, &mut cx).await;
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

        // Open window with default options
        cx.open_window(
            WindowOptions {
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
            },
            |window, cx| {
                window.set_window_title(APP_NAME);
                window.set_app_id(APP_ID);

                #[cfg(not(target_os = "linux"))]
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
                        let metadata = if let Ok(Some(metadata)) =
                            client.database().metadata(public_key).await
                        {
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
                    if let Ok(Some(_profile)) = rx.await {
                        // TODO: Implement login
                    } else {
                        _ = cx.update_window(handle, |_, window, cx| {
                            window.replace_root(cx, |window, cx| {
                                Root::new(onboarding::init(window, cx).into(), window, cx)
                            });
                        });
                    }
                })
                .detach();

                cx.spawn(|cx| async move {
                    while let Ok(signal) = event_rx.recv().await {
                        cx.update(|cx| {
                            match signal {
                                Signal::Eose => {
                                    if let Some(chats) = ChatRegistry::global(cx) {
                                        chats.update(cx, |this, cx| this.load_chat_rooms(cx))
                                    }
                                }
                                Signal::Event(event) => {
                                    if let Some(chats) = ChatRegistry::global(cx) {
                                        chats.update(cx, |this, cx| this.push_message(event, cx))
                                    }
                                }
                            };
                        })
                        .ok();
                    }
                })
                .detach();

                root
            },
        )
        .expect("System error. Please re-open the app.");
    });
}

async fn sync_metadata(client: &Client, buffer: HashSet<PublicKey>) {
    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
    let filter = Filter::new()
        .authors(buffer.iter().cloned())
        .kind(Kind::Metadata)
        .limit(buffer.len());

    if let Err(e) = client.subscribe(filter, Some(opts)).await {
        error!("Subscribe error: {e}");
    }
}

async fn restore_window(is_login: bool, cx: &mut AsyncApp) -> anyhow::Result<()> {
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

    if is_login {
        _ = cx.open_window(opts, |window, cx| {
            window.set_window_title(APP_NAME);
            window.set_app_id(APP_ID);

            #[cfg(not(target_os = "linux"))]
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            cx.new(|cx| Root::new(app::init(window, cx).into(), window, cx))
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
