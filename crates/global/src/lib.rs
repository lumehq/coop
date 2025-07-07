use std::fs;
use std::sync::OnceLock;

use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use paths::nostr_file;

use crate::paths::support_dir;

pub mod constants;
pub mod paths;

/// Signals sent through the global event channel to notify UI components
#[derive(Debug, Clone)]
pub enum NostrSignal {
    /// Received a new metadata event from Relay Pool
    Metadata(Event),

    /// Received a new gift wrap event from Relay Pool
    GiftWrap(Event),

    /// Finished processing all gift wrap events
    Finish,

    /// Partially finished processing all gift wrap events
    PartialFinish,

    /// Receives EOSE response from relay pool
    Eose(SubscriptionId),

    /// Notice from Relay Pool
    Notice(String),

    /// Application update event received
    AppUpdate(Event),
}

static NOSTR_CLIENT: OnceLock<Client> = OnceLock::new();
static FIRST_RUN: OnceLock<bool> = OnceLock::new();

pub fn nostr_client() -> &'static Client {
    NOSTR_CLIENT.get_or_init(|| {
        // rustls uses the `aws_lc_rs` provider by default
        // This only errors if the default provider has already
        // been installed. We can ignore this `Result`.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let opts = ClientOptions::new().gossip(true);
        let lmdb = NostrLMDB::open(nostr_file()).expect("Database is NOT initialized");

        ClientBuilder::default().database(lmdb).opts(opts).build()
    })
}

pub fn first_run() -> &'static bool {
    FIRST_RUN.get_or_init(|| {
        let flag = support_dir().join(format!(".{}-first_run", env!("CARGO_PKG_VERSION")));

        if !flag.exists() {
            if fs::write(&flag, "").is_err() {
                return false;
            }
            true // First run
        } else {
            false // Not first run
        }
    })
}
