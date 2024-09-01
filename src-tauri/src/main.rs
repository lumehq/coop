// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
use border::WebviewWindowExt as WebviewWindowExtAlt;
use nostr_sdk::prelude::*;
use specta_typescript::Typescript;
use std::env;
use std::{
	collections::HashMap,
	fs,
	io::{self, BufRead},
	str::FromStr,
	time::Duration,
};
use tauri::{async_runtime::Mutex, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_decorum::WebviewWindowExt;
use tauri_specta::{collect_commands, Builder};

use commands::{account::*, chat::*, relay::*};

mod commands;

pub struct Nostr {
	client: Client,
	bootstrap_relays: Mutex<Vec<String>>,
	inbox_relays: Mutex<HashMap<PublicKey, Vec<String>>>,
}

fn main() {
	#[cfg(target_os = "linux")]
	std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

	let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
		get_bootstrap_relays,
		set_bootstrap_relays,
		collect_inbox_relays,
		set_inbox_relays,
		connect_inbox_relays,
		disconnect_inbox_relays,
		login,
		create_account,
		import_account,
		connect_account,
		delete_account,
		reset_password,
		get_accounts,
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
			// This is also required if you want to use events
			builder.mount_events(app);

			let handle = app.handle();
			let main_window = app.get_webview_window("main").unwrap();

			// Open devtools
			#[cfg(debug_assertions)]
			main_window.open_devtools();

			// Set custom decoration
			#[cfg(target_os = "windows")]
			main_window.create_overlay_titlebar().unwrap();

			// Set traffic light inset
			#[cfg(target_os = "macos")]
			main_window.set_traffic_lights_inset(12.0, 18.0).unwrap();

			// Workaround for reset traffic light when window resized
			#[cfg(target_os = "macos")]
			let win_ = main_window.clone();
			#[cfg(target_os = "macos")]
			main_window.on_window_event(move |event| {
				if let tauri::WindowEvent::ThemeChanged(_) = event {
					win_.set_traffic_lights_inset(12.0, 18.0).unwrap();
				}
			});

			// Restore native border
			#[cfg(target_os = "macos")]
			main_window.add_border(None);

			let (client, bootstrap_relays) = tauri::async_runtime::block_on(async move {
				// Create data folder if not exist
				let dir = handle.path().app_config_dir().expect("App config directory not found.");
				let _ = fs::create_dir_all(dir.clone());

				// Setup database
				let database = SQLiteDatabase::open(dir.join("nostr.db")).await.expect("Error.");

				// Setup nostr client
				let opts = Options::new()
					.autoconnect(true)
					.timeout(Duration::from_secs(30))
					.send_timeout(Some(Duration::from_secs(2)))
					.connection_timeout(Some(Duration::from_secs(10)));

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
									println!("Connecting to relay...: {} - {}", relay, meta);
									let opts = if meta == RelayMetadata::Read {
										RelayOptions::new().read(true).write(false)
									} else {
										RelayOptions::new().write(true).read(false)
									};
									let _ = client.add_relay_with_opts(relay, opts).await;
								}
								Err(_) => {
									println!("Connecting to relay...: {}", relay);
									let _ = client.add_relay(relay).await;
								}
							}
						}
					}
				}

				let bootstrap_relays = client
					.relays()
					.await
					.keys()
					.map(|item| item.to_string())
					.collect::<Vec<String>>();

				(client, bootstrap_relays)
			});

			// Create global state
			app.manage(Nostr {
				client,
				bootstrap_relays: Mutex::new(bootstrap_relays),
				inbox_relays: Mutex::new(HashMap::new()),
			});

			Ok(())
		})
		.enable_macos_default_menu(false)
		.plugin(tauri_plugin_fs::init())
		.plugin(tauri_plugin_prevent_default::init())
		.plugin(tauri_plugin_process::init())
		.plugin(tauri_plugin_updater::Builder::new().build())
		.plugin(tauri_plugin_os::init())
		.plugin(tauri_plugin_clipboard_manager::init())
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_decorum::init())
		.plugin(tauri_plugin_notification::init())
		.plugin(tauri_plugin_shell::init())
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
