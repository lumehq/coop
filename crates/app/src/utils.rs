use chrono::{Duration, Local, TimeZone};
use nostr_sdk::prelude::*;

pub fn get_room_id(author: &PublicKey, tags: &Tags) -> String {
    // Get all public keys
    let mut pubkeys: Vec<PublicKey> = tags.public_keys().copied().collect();
    // Add author to public keys list
    pubkeys.insert(0, *author);

    let hex: Vec<String> = pubkeys
        .iter()
        .map(|m| {
            let hex = m.to_hex();
            let split = &hex[..6];

            split.to_owned()
        })
        .collect();

    hex.join("-")
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
