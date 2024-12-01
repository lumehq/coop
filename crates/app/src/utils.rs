use chrono::{Local, TimeZone};
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;

pub fn get_all_accounts_from_keyring() -> Vec<PublicKey> {
    let search = Search::new().expect("Keyring not working.");
    let results = search.by_service("Coop Safe Storage");
    let list = List::list_credentials(&results, Limit::All);
    let accounts: Vec<PublicKey> = list
        .split_whitespace()
        .filter(|v| v.starts_with("npub1") && !v.ends_with("coop"))
        .filter_map(|i| PublicKey::from_bech32(i).ok())
        .collect();

    accounts
}

pub fn ago(time: u64) -> String {
    let now = Local::now();
    let input_time = Local.timestamp_opt(time as i64, 0).unwrap();
    let diff = (now - input_time).num_hours();

    if diff < 24 {
        let duration = now.signed_duration_since(input_time);
        format!("{} ago", duration.num_hours())
    } else {
        input_time.format("%b %d").to_string()
    }
}
