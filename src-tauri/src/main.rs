// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
use border::WebviewWindowExt as WebviewWindowExtAlt;
use commands::tray::setup_tray_icon;
use nostr_sdk::prelude::*;
use specta_typescript::Typescript;
use std::{
	collections::HashMap,
	env, fs,
	io::{self, BufRead},
	str::FromStr,
};
use tauri::{async_runtime::Mutex, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_decorum::WebviewWindowExt;
use tauri_specta::{collect_commands, Builder};

use commands::{account::*, chat::*, relay::*};

mod commands;

pub struct Nostr {
	client: Client,
	inbox_relays: Mutex<HashMap<PublicKey, Vec<String>>>,
}

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

	tauri::Builder::default()
		.invoke_handler(builder.invoke_handler())
		.setup(move |app| {
			let handle = app.handle();
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
				let database = NostrLMDB::open(dir.join("nostr-lmdb"))
					.expect("Error: cannot create database.");

				// Setup nostr client
				let opts = Options::new().gossip(true).autoconnect(true);
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

				client
			});

			// Create global state
			app.manage(Nostr { client, inbox_relays: Mutex::new(HashMap::new()) });

			Ok(())
		})
		.plugin(prevent_default())
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
