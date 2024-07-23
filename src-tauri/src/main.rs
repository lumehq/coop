// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use border::WebviewWindowExt as WebviewWindowExtAlt;
use commands::account::{get_accounts, get_profile, login};
use nostr_sdk::prelude::*;
use serde::Serialize;
use std::{fs, sync::Mutex};
use tauri::Manager;
use tauri_plugin_decorum::WebviewWindowExt;

mod commands;

#[derive(Serialize)]
pub struct Nostr {
	#[serde(skip_serializing)]
	client: Client,
	contact_list: Mutex<Vec<Contact>>,
}

fn main() {
	let mut ctx = tauri::generate_context!();
	let invoke_handler = {
		let builder = tauri_specta::ts::builder().commands(tauri_specta::collect_commands![
			get_accounts,
			login,
			get_profile
		]);

		#[cfg(debug_assertions)]
		let builder = builder.path("../src/commands.ts");

		builder.build().unwrap()
	};

	tauri::Builder::default()
		.setup(|app| {
			#[cfg(not(target_os = "linux"))]
			let main_window = app.get_webview_window("main").unwrap();

			// Set custom decoration
			main_window.create_overlay_titlebar().unwrap();

			// Restore native border
			#[cfg(target_os = "macos")]
			main_window.add_border(None);

			tauri::async_runtime::block_on(async move {
				// Create data folder if not exist
				let dir = app.path().config_dir().expect("Config Directory not found.");
				let _ = fs::create_dir_all(dir.join("Coop/"));

				// Setup database
				let database = SQLiteDatabase::open(dir.join("Coop/coop.db")).await;

				// Setup nostr client
				let client = match database {
					Ok(db) => ClientBuilder::default().database(db).build(),
					Err(_) => ClientBuilder::default().build(),
				};

				// Add bootstrap relay
				let _ = client.add_relay("wss://relay.damus.io/").await;
				let _ = client.add_relay("wss://relay.nostr.net/").await;

				// Connect
				client.connect().await;

				// Create global state
				app.handle().manage(Nostr { client, contact_list: Mutex::new(vec![]) })
			});

			Ok(())
		})
		.enable_macos_default_menu(false)
		.plugin(tauri_plugin_os::init())
		.plugin(tauri_plugin_clipboard_manager::init())
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_decorum::init())
		.plugin(tauri_plugin_shell::init())
		.invoke_handler(invoke_handler)
		.run(ctx)
		.expect("error while running tauri application");
}
