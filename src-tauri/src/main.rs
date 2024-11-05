// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
use border::WebviewWindowExt as WebviewWindowExtAlt;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use specta_typescript::Typescript;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, BufRead},
    str::FromStr,
};
use tauri::{Emitter, Listener, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_decorum::WebviewWindowExt;
use tauri_specta::{collect_commands, Builder};
use tokio::{sync::RwLock, time::sleep, time::Duration};

use commands::{account::*, chat::*, relay::*, tray::*};

mod commands;

pub struct Nostr {
    client: Client,
    queue: RwLock<HashSet<PublicKey>>,
    inbox_relays: RwLock<HashMap<PublicKey, Vec<String>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Payload {
    id: String,
}

#[derive(Clone, Serialize)]
pub struct EventPayload {
    event: String, // JSON String
    sender: String,
}

pub const QUEUE_DELAY: u64 = 300;
pub const SUBSCRIPTION_ID: &str = "inbox";

fn main() {
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        get_bootstrap_relays,
        set_bootstrap_relays,
        get_inbox_relays,
        set_inbox_relays,
        ensure_inbox_relays,
        connect_inbox_relays,
        disconnect_inbox_relays,
        login,
        create_account,
        import_account,
        connect_account,
        delete_account,
        reset_password,
        get_accounts,
        get_current_account,
        get_metadata,
        get_contact_list,
        get_chats,
        get_chat_messages,
        send_message,
    ]);

    #[cfg(debug_assertions)]
    builder
        .export(Typescript::default(), "../src/commands.ts")
        .expect("Failed to export typescript bindings");

    #[cfg(debug_assertions)]
    let tauri_builder = tauri::Builder::default().plugin(tauri_plugin_devtools::init());

    #[cfg(not(debug_assertions))]
    let tauri_builder = tauri::Builder::default();

    tauri_builder
		.invoke_handler(builder.invoke_handler())
		.setup(move |app| {
			let handle = app.handle();
			let handle_clone = handle.clone();
			let handle_clone_child = handle_clone.clone();

			// Setup tray icon
			let _ = setup_tray_icon(handle);

			#[cfg(not(target_os = "linux"))]
			let main_window = app.get_webview_window("main").unwrap();

			// Set custom decoration
			#[cfg(target_os = "windows")]
			main_window.create_overlay_titlebar().unwrap();

			// Set traffic light inset
			#[cfg(target_os = "macos")]
			main_window.set_traffic_lights_inset(12.0, 18.0).unwrap();

			// Restore native border
			#[cfg(target_os = "macos")]
			main_window.add_border(None);

			let client = tauri::async_runtime::block_on(async move {
				// Get config directory
				let dir =
					handle.path().app_config_dir().expect("Error: config directory not found.");

				// Create config directory if not exist
				let _ = fs::create_dir_all(&dir);

				// Setup database
				let database = NostrLMDB::open(dir.join("nostr-lmdb")).expect("Error: cannot create database.");

				// Setup nostr client
				let opts = Options::new().gossip(true).automatic_authentication(false).max_avg_latency(Duration::from_millis(500));
				let client = ClientBuilder::default().opts(opts).database(database).build();

				// Add bootstrap relays
				if let Ok(path) = handle
					.path()
					.resolve("resources/relays.txt", tauri::path::BaseDirectory::Resource)
				{
					let file = std::fs::File::open(&path).unwrap();
					let lines = io::BufReader::new(file).lines();

					// Add bootstrap relays to relay pool
					for line in lines.map_while(Result::ok) {
						if let Some((relay, option)) = line.split_once(',') {
							match RelayMetadata::from_str(option) {
								Ok(meta) => {
									let opts = if meta == RelayMetadata::Read {
										RelayOptions::new().read(true).write(false)
									} else {
										RelayOptions::new().write(true).read(false)
									};
									let _ = client.pool().add_relay(relay, opts).await;
								}
								Err(_) => {
									let _ = client.add_relay(relay).await;
								}
							}
						}
					}
				}

				let _ = client.add_discovery_relay("wss://user.kindpag.es/").await;

				// Connect
				client.connect().await;

				client
			});

			// Create global state
			app.manage(Nostr {
				client,
				queue: RwLock::new(HashSet::new()),
				inbox_relays: RwLock::new(HashMap::new()),
			});

			// Listen for request metadata
			app.listen_any("request_metadata", move |event| {
				let payload = event.payload();
				let parsed_payload: Payload = serde_json::from_str(payload).expect("Parse failed");
				let handle = handle_clone.to_owned();

				tauri::async_runtime::spawn(async move {
					let state = handle.state::<Nostr>();
					let client = &state.client;

					if let Ok(public_key) = PublicKey::parse(parsed_payload.id) {
						let mut write_queue = state.queue.write().await;
						write_queue.insert(public_key);
					};

					// Wait for [QUEUE_DELAY]
					sleep(Duration::from_millis(QUEUE_DELAY)).await;

					let read_queue = state.queue.read().await;

					if !read_queue.is_empty() {
						let authors: HashSet<PublicKey> = read_queue.iter().copied().collect();

						let filter = Filter::new().authors(authors).kind(Kind::Metadata).limit(200);

						let opts = SubscribeAutoCloseOptions::default()
							.filter(FilterOptions::WaitDurationAfterEOSE(Duration::from_secs(2)));

						// Drop queue, you don't need it at this time anymore
						drop(read_queue);
						// Clear queue
						let mut write_queue = state.queue.write().await;
						write_queue.clear();

						if let Err(e) = client.subscribe(vec![filter], Some(opts)).await {
							println!("Subscribe error: {}", e);
						}
					}
				});
			});

			// Run a thread for handle notification
			tauri::async_runtime::spawn(async move {
				let handle = handle_clone_child.to_owned();
				let state = handle.state::<Nostr>();
				let client = &state.client;

				// Generate a fake sig for rumor event.
				// TODO: Find better way to save unsigned event to database.
				let fake_sig = Signature::from_str("f9e79d141c004977192d05a86f81ec7c585179c371f7350a5412d33575a2a356433f58e405c2296ed273e2fe0aafa25b641e39cc4e1f3f261ebf55bce0cbac83").unwrap();

				let _ = client
					.handle_notifications(|notification| async {
						#[allow(clippy::collapsible_match)]
						if let RelayPoolNotification::Message { message, .. } = notification {
							if let RelayMessage::Event { event, .. } = message {
								if event.kind == Kind::GiftWrap {
									if let Ok(UnwrappedGift { rumor, sender }) =
										client.unwrap_gift_wrap(&event).await
									{
										let mut rumor_clone = rumor.clone();

										// Compute event id if not exist
										rumor_clone.ensure_id();

										let ev = Event::new(
											rumor_clone.id.unwrap(), // unwrap() must be fine
											rumor_clone.pubkey,
											rumor_clone.created_at,
											rumor_clone.kind,
											rumor_clone.tags,
											rumor_clone.content,
											fake_sig,
										);

										// Save rumor to database to further query
										if let Err(e) = client.database().save_event(&ev).await {
											println!("[save event] error: {}", e)
										}

										// Emit new event to frontend
										if let Err(e) = handle.emit(
											"event",
											EventPayload {
												event: rumor.as_json(),
												sender: sender.to_hex(),
											},
										) {
											println!("[emit] error: {}", e)
										}
									}
								} else if event.kind == Kind::Metadata {
                                    if let Err(e) = handle.emit("metadata", event.as_json()) {
                                        println!("Emit error: {}", e)
                                    }
                                }
							}
						}
						Ok(false)
					})
					.await;
			});

			Ok(())
		})
		.plugin(prevent_default())
		.plugin(tauri_plugin_store::Builder::default().build())
		.plugin(tauri_plugin_fs::init())
		.plugin(tauri_plugin_process::init())
		.plugin(tauri_plugin_updater::Builder::new().build())
		.plugin(tauri_plugin_os::init())
		.plugin(tauri_plugin_clipboard_manager::init())
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_decorum::init())
		.plugin(tauri_plugin_notification::init())
		.plugin(tauri_plugin_shell::init())
		.build(tauri::generate_context!())
		.expect("error while running tauri application")
		.run(|_app_handle, event| {
			if let tauri::RunEvent::ExitRequested { api, .. } = event {
				api.prevent_exit();
			}
		});
}

#[cfg(debug_assertions)]
fn prevent_default() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_prevent_default::Flags;

    tauri_plugin_prevent_default::Builder::new()
        .with_flags(Flags::all().difference(Flags::CONTEXT_MENU))
        .build()
}

#[cfg(not(debug_assertions))]
fn prevent_default() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_prevent_default::Builder::new().build()
}
