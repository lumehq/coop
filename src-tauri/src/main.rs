// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
use border::WebviewWindowExt as WebviewWindowExtAlt;
use nostr_sdk::prelude::*;
use std::{collections::HashMap, fs, time::Duration};
use tauri::{async_runtime::Mutex, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_decorum::WebviewWindowExt;

use commands::{account::*, chat::*};

mod commands;

pub struct Nostr {
	client: Client,
	inbox_relays: Mutex<HashMap<PublicKey, Vec<String>>>,
}

// TODO: Allow user config bootstrap relays.
pub const BOOTSTRAP_RELAYS: [&str; 2] = ["wss://relay.damus.io/", "wss://relay.nostr.net/"];

fn main() {
	let invoke_handler = {
		let builder = tauri_specta::ts::builder().commands(tauri_specta::collect_commands![
			login,
			delete_account,
			create_account,
			import_key,
			connect_account,
			get_accounts,
			get_metadata,
			get_contact_list,
			get_chats,
			get_chat_messages,
			get_inbox,
			set_inbox,
			connect_inbox,
			disconnect_inbox,
			send_message,
		]);

		#[cfg(debug_assertions)]
		let builder = builder.path("../src/commands.ts");

		builder.build().unwrap()
	};

	#[cfg(debug_assertions)]
	let builder = tauri::Builder::default().plugin(tauri_plugin_devtools::init());
	#[cfg(not(debug_assertions))]
	let builder = tauri::Builder::default();

	builder
		.setup(|app| {
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
				if let tauri::WindowEvent::Resized(_) = event {
					win_.set_traffic_lights_inset(12.0, 18.0).unwrap();
				}
				if let tauri::WindowEvent::ThemeChanged(_) = event {
					win_.set_traffic_lights_inset(12.0, 18.0).unwrap();
				}
			});

			// Restore native border
			#[cfg(target_os = "macos")]
			main_window.add_border(None);

			let client = tauri::async_runtime::block_on(async move {
				// Create data folder if not exist
				let dir = handle.path().app_config_dir().expect("App config directory not found.");
				let _ = fs::create_dir_all(dir.clone());

				// Setup database
				let database = SQLiteDatabase::open(dir.join("nostr.db")).await.expect("Error.");

				// Setup nostr client
				let opts = Options::new()
					.timeout(Duration::from_secs(40))
					.send_timeout(Some(Duration::from_secs(10)))
					.connection_timeout(Some(Duration::from_secs(10)));

				let client = ClientBuilder::default().opts(opts).database(database).build();

				// Add bootstrap relay
				let _ = client.add_relays(BOOTSTRAP_RELAYS).await;

				// Connect
				client.connect().await;

				client
			});

			// Create global state
			app.manage(Nostr { client, inbox_relays: Mutex::new(HashMap::new()) });

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
		.plugin(tauri_plugin_shell::init())
		.invoke_handler(invoke_handler)
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
