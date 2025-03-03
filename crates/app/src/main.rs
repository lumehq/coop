use anyhow::anyhow;
use asset::Assets;
use chats::registry::ChatRegistry;
use device::Device;
use futures::{select, FutureExt};
use global::{
    constants::{
        ALL_MESSAGES_SUB_ID, APP_NAME, BOOTSTRAP_RELAYS, DEVICE_ANNOUNCEMENT_KIND,
        DEVICE_REQUEST_KIND, DEVICE_RESPONSE_KIND, NEW_MESSAGE_SUB_ID,
    },
    get_client, get_device_keys,
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
    nips::nip59::UnwrappedGift, pool::prelude::ReqExitPolicy, Event, Filter, Keys, Kind, PublicKey,
    RelayMessage, RelayPoolNotification, SubscribeAutoCloseOptions, SubscriptionId, TagKind,
};
use smol::Timer;
use std::{collections::HashSet, mem, sync::Arc, time::Duration};
use ui::Root;
use views::{onboarding, startup};

mod asset;
mod device;
mod views;

actions!(coop, [Quit]);

#[derive(Debug)]
enum Signal {
    /// Receive event
    Event(Event),
    /// Receive request master key event
    RequestMasterKey((PublicKey, Option<String>)),
    /// Receive approve master key event
    ReceiveMasterKey(Event),
    /// Receive EOSE
    Eose,
}

fn main() {
    // Enable logging
    tracing_subscriber::fmt::init();

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
            // Fix crash on startup
            // TODO: why this is needed?
            _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

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
                                    if let Ok(gift) = handle_gift_wrap(&event).await {
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
                                                log::error!("Failed to save event: {}", e);
                                            }

                                            // Send all pubkeys to the batch
                                            if let Err(e) = batch_tx.send(pubkeys).await {
                                                log::error!(
                                                    "Failed to send pubkeys to batch: {}",
                                                    e
                                                )
                                            }

                                            // Send this event to the GPUI
                                            if new_id == *subscription_id {
                                                if let Err(e) =
                                                    event_tx.send(Signal::Event(event)).await
                                                {
                                                    log::error!(
                                                        "Failed to send event to GPUI: {}",
                                                        e
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                                Kind::ContactList => {
                                    let pubkeys =
                                        event.tags.public_keys().copied().collect::<HashSet<_>>();

                                    handle_metadata(pubkeys).await;
                                }
                                Kind::Custom(DEVICE_REQUEST_KIND) => {
                                    let public_key = event
                                        .tags
                                        .find(TagKind::custom("pubkey"))
                                        .and_then(|tag| tag.content())
                                        .and_then(|content| PublicKey::parse(content).ok());

                                    let name = event
                                        .tags
                                        .find(TagKind::Client)
                                        .and_then(|tag| tag.content())
                                        .map(|content| content.to_string());

                                    if let Some(public_key) = public_key {
                                        if let Err(e) = event_tx
                                            .send(Signal::RequestMasterKey((public_key, name)))
                                            .await
                                        {
                                            log::error!("Failed to send: {}", e)
                                        };
                                    }
                                }
                                Kind::Custom(DEVICE_RESPONSE_KIND) => {
                                    if let Err(e) = event_tx
                                        .send(Signal::ReceiveMasterKey(event.into_owned()))
                                        .await
                                    {
                                        log::error!("Failed to send: {}", e)
                                    };
                                }
                                Kind::Custom(DEVICE_ANNOUNCEMENT_KIND) => {
                                    log::info!("Device announcement received")
                                }
                                _ => {}
                            }
                        }
                        RelayMessage::EndOfStoredEvents(subscription_id) => {
                            if all_id == *subscription_id {
                                if let Err(e) = event_tx.send(Signal::Eose).await {
                                    log::error!("Failed to send eose: {}", e)
                                };
                            }
                        }
                        _ => {}
                    }
                }
            }
        })
        .detach();

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
            ..Default::default()
        };

        // Open a window with default options
        cx.open_window(opts, |window, cx| {
            let handle = window.window_handle();

            // Spawn a task to handle credentials
            cx.spawn(|cx| async move {
                /*
                if let Ok(Some((_, vec))) = read_credentials.await {
                    let Ok(content) = String::from_utf8(vec) else {
                        return;
                    };

                    let Ok(uri) = NostrConnectURI::parse(content) else {
                        return;
                    };

                    let keys = Keys::generate();

                    if let Ok(signer) = NostrConnect::new(uri, keys, Duration::from_secs(300), None)
                    {
                        if device::init(signer, &cx).await.is_ok() {
                            cx.update(|cx| {
                                handle
                                    .update(cx, |_, window, cx| {
                                        window.replace_root(cx, |window, cx| {
                                            Root::new(app::init(window, cx).into(), window, cx)
                                        });
                                    })
                                    .expect("Window is closed. Please restart the application.")
                            })
                            .ok();
                        }
                    }
                    return;
                }
                */

                cx.update(|cx| {
                    handle
                        .update(cx, |_, window, cx| {
                            window.replace_root(cx, |window, cx| {
                                Root::new(onboarding::init(window, cx).into(), window, cx)
                            });
                        })
                        .expect("Window is closed. Please restart the application.")
                })
                .ok();
            })
            .detach();

            // Spawn a task to handle events from nostr channel
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
                            Signal::ReceiveMasterKey(event) => {
                                if let Some(device) = Device::global(cx) {
                                    _ = handle.update(cx, |_, window, cx| {
                                        device.update(cx, |this, cx| {
                                            this.handle_response(&event, window, cx);
                                        });
                                    });
                                }
                            }
                            Signal::RequestMasterKey(user) => {
                                if let Some(device) = Device::global(cx) {
                                    _ = handle.update(cx, |_, window, cx| {
                                        device.update(cx, |this, cx| {
                                            this.handle_request(user, window, cx);
                                        });
                                    });
                                }
                            }
                        };
                    })
                    .ok();
                }
            })
            .detach();

            cx.new(|cx| Root::new(startup::init(window, cx).into(), window, cx))
        })
        .expect("Failed to open window. Please restart the application.");
    });
}

async fn handle_gift_wrap(gift_wrap: &Event) -> Result<UnwrappedGift, anyhow::Error> {
    let client = get_client();

    if let Some(device) = get_device_keys().await {
        // Try to unwrap with the device keys first
        match UnwrappedGift::from_gift_wrap(&device, gift_wrap).await {
            Ok(event) => Ok(event),
            Err(_) => {
                // Try to unwrap again with the user's signer
                let signer = client.signer().await?;
                let event = UnwrappedGift::from_gift_wrap(&signer, gift_wrap).await?;
                Ok(event)
            }
        }
    } else {
        Err(anyhow!("Signer not found"))
    }
}

async fn handle_metadata(buffer: HashSet<PublicKey>) {
    let client = get_client();

    let opts = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::ExitOnEOSE)
        .idle_timeout(Some(Duration::from_secs(2)));

    let filter = Filter::new()
        .authors(buffer.iter().cloned())
        .limit(buffer.len() * 2)
        .kinds(vec![Kind::Metadata, Kind::Custom(DEVICE_ANNOUNCEMENT_KIND)]);

    if let Err(e) = client.subscribe(filter, Some(opts)).await {
        log::error!("Failed to sync metadata: {e}");
    }
}

fn quit(_: &Quit, cx: &mut App) {
    log::info!("Gracefully quitting the application . . .");
    cx.quit();
}
