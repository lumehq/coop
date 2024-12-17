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

pub fn show_npub(public_key: PublicKey, len: usize) -> String {
    let bech32 = public_key.to_bech32().unwrap_or_default();
    let separator = " ... ";

    let sep_len = separator.len();
    let chars_to_show = len - sep_len;
    let front_chars = (chars_to_show + 1) / 2; // ceil
    let back_chars = chars_to_show / 2; // floor

    format!(
        "{}{}{}",
        &bech32[..front_chars],
        separator,
        &bech32[bech32.len() - back_chars..]
    )
}

pub fn ago(time: u64) -> String {
    let now = Local::now();
    let input_time = Local.timestamp_opt(time as i64, 0).unwrap();
    let diff = (now - input_time).num_hours();

    if diff == 0 {
        "now".to_owned()
    } else if diff < 24 {
        let duration = now.signed_duration_since(input_time);
        format!("{} hours ago", duration.num_hours())
    } else {
        input_time.format("%b %d").to_string()
    }
}
