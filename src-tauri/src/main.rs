// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use border::WebviewWindowExt as WebviewWindowExtAlt;
use nostr_sdk::prelude::*;
use serde::Serialize;
use std::{fs, sync::Mutex, time::Duration};
use tauri::Manager;
use tauri_plugin_decorum::WebviewWindowExt;

use commands::{account::*, chat::*};

mod commands;
mod common;

#[derive(Serialize)]
pub struct Nostr {
	#[serde(skip_serializing)]
	client: Client,
	contact_list: Mutex<Vec<Contact>>,
}

fn main() {
	let invoke_handler = {
		let builder = tauri_specta::ts::builder().commands(tauri_specta::collect_commands![
			login,
			create_account,
			import_key,
			connect_account,
			get_accounts,
			get_metadata,
			get_inboxes,
			get_chats,
			get_chat_messages,
			send_message,
			subscribe_to,
			unsubscribe,
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

			tauri::async_runtime::block_on(async move {
				// Create data folder if not exist
				let dir = app.path().config_dir().expect("Config Directory not found.");
				let _ = fs::create_dir_all(dir.join("Coop/"));

				// Setup database
				let database = SQLiteDatabase::open(dir.join("Coop/coop.db")).await;

				// Config
				let opts = Options::new()
					.autoconnect(true)
					.timeout(Duration::from_secs(5))
					.send_timeout(Some(Duration::from_secs(5)))
					.connection_timeout(Some(Duration::from_secs(20)));

				// Setup nostr client
				let client = match database {
					Ok(db) => ClientBuilder::default().opts(opts).database(db).build(),
					Err(_) => ClientBuilder::default().opts(opts).build(),
				};

				// Add bootstrap relay
				let _ =
					client.add_relays(["wss://relay.damus.io/", "wss://relay.nostr.net/"]).await;

				// Create global state
				app.handle().manage(Nostr { client, contact_list: Mutex::new(vec![]) })
			});

			Ok(())
		})
		.enable_macos_default_menu(false)
		.plugin(tauri_plugin_prevent_default::init())
		.plugin(tauri_plugin_os::init())
		.plugin(tauri_plugin_clipboard_manager::init())
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_decorum::init())
		.plugin(tauri_plugin_shell::init())
		.invoke_handler(invoke_handler)
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
