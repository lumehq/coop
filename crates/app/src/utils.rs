use chrono::{Duration, Local, TimeZone};
use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;

use crate::constants::KEYRING_SERVICE;

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

pub fn get_keys_by_account(public_key: PublicKey) -> Result<Keys, anyhow::Error> {
    let bech32 = public_key.to_bech32()?;
    let entry = Entry::new(KEYRING_SERVICE, &bech32)?;
    let password = entry.get_password()?;
    let keys = Keys::parse(&password)?;

    Ok(keys)
}

pub fn get_room_id(owner: &PublicKey, public_keys: &[PublicKey]) -> String {
    let hex: Vec<String> = public_keys
        .iter()
        .map(|m| {
            let hex = m.to_hex();
            let split = &hex[..6];

            split.to_owned()
        })
        .collect();
    let mems = hex.join("-");

    format!("{}-{}", &owner.to_hex()[..6], mems)
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

    if diff < 24 {
        let duration = now.signed_duration_since(input_time);
        format_duration(duration)
    } else {
        input_time.format("%b %d").to_string()
    }
}

pub fn format_duration(duration: Duration) -> String {
    if duration.num_seconds() < 60 {
        "now".to_string()
    } else if duration.num_minutes() == 1 {
        "1m".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m", duration.num_minutes())
    } else if duration.num_hours() == 1 {
        "1h".to_string()
    } else if duration.num_hours() < 24 {
        format!("{}h", duration.num_hours())
    } else if duration.num_days() == 1 {
        "1d".to_string()
    } else {
        format!("{}d", duration.num_days())
    }
}
