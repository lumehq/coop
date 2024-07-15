use std::fs;

use dirs::config_dir;
use freya::prelude::*;
use nostr_sdk::{Client, ClientBuilder, RelayOptions, SQLiteDatabase, UnsignedEvent};
use tokio::sync::OnceCell;

pub static CHATS: GlobalSignal<Vec<UnsignedEvent>> = Signal::global(Vec::new);
pub static MESSAGES: GlobalSignal<Vec<UnsignedEvent>> = Signal::global(Vec::new);
pub static CURRENT_USER: GlobalSignal<String> = Signal::global(String::new);

pub static CLIENT: OnceCell<Client> = OnceCell::const_new();

pub async fn get_client() -> &'static Client {
	CLIENT
		.get_or_init(|| async {
			// Create data folder if not exist
			let config_dir = config_dir().unwrap();
			let _ = fs::create_dir_all(config_dir.join("Coop/"));

			// Setup database
			let database = SQLiteDatabase::open(config_dir.join("Coop/coop.db")).await;

			// Config
			let relay_opts = RelayOptions::new().write(false).read(true);

			// Setup nostr client
			let client = match database {
				Ok(db) => ClientBuilder::default().database(db).build(),
				Err(_) => ClientBuilder::default().build(),
			};

			if client
				.add_relay_with_opts("wss://relay.damus.io", relay_opts.clone())
				.await
				.is_ok()
			{
				println!("connecting to wss://relay.damus.io ...")
			}

			if client
				.add_relay_with_opts("wss://relay.nostr.net", relay_opts)
				.await
				.is_ok()
			{
				println!("connecting to wss://relay.nostr.net ...")
			}

			// Connect
			client.connect().await;

			client
		})
		.await
}
