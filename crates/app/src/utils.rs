use chrono::{Duration, Local, TimeZone};
use nostr_sdk::prelude::*;
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use crate::{constants::NIP96_SERVER, get_client};

pub async fn nip96_upload(file: Vec<u8>) -> anyhow::Result<Url, anyhow::Error> {
    let client = get_client();
    let signer = client.signer().await?;
    let server_url = Url::parse(NIP96_SERVER)?;

    let config: ServerConfig = nip96::get_server_config(server_url, None).await?;
    let url = nip96::upload_data(&signer, &config, file, None, None).await?;

    Ok(url)
}

pub fn room_hash(tags: &Tags) -> u64 {
    let pubkeys: Vec<PublicKey> = tags.public_keys().copied().collect();
    let mut hasher = DefaultHasher::new();
    // Generate unique hash
    pubkeys.hash(&mut hasher);

    hasher.finish()
}

pub fn compare<T>(a: &[T], b: &[T]) -> bool
where
    T: Eq + Hash,
{
    let a: HashSet<_> = a.iter().collect();
    let b: HashSet<_> = b.iter().collect();

    a == b
}

pub fn shorted_public_key(public_key: PublicKey) -> String {
    let pk = public_key.to_string();
    format!("{}:{}", &pk[0..4], &pk[pk.len() - 4..])
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

pub fn ago(time: Timestamp) -> String {
    let now = Local::now();
    let input_time = Local.timestamp_opt(time.as_u64() as i64, 0).unwrap();
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
