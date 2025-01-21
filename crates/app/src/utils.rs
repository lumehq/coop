use crate::{constants::NIP96_SERVER, get_client};
use chrono::{Datelike, Local, TimeZone};
use nostr_sdk::prelude::*;
use rnglib::{Language, RNG};
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

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

pub fn random_name(length: usize) -> String {
    let rng = RNG::from(&Language::Roman);
    rng.generate_names(length, true).join("-").to_lowercase()
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

pub fn message_ago(time: Timestamp) -> String {
    let now = Local::now();
    let input_time = Local.timestamp_opt(time.as_u64() as i64, 0).unwrap();
    let diff = (now - input_time).num_hours();

    if diff < 24 {
        let duration = now.signed_duration_since(input_time);

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
    } else {
        input_time.format("%b %d").to_string()
    }
}

pub fn message_time(time: Timestamp) -> String {
    let now = Local::now();
    let input_time = Local.timestamp_opt(time.as_u64() as i64, 0).unwrap();

    if input_time.day() == now.day() {
        format!("Today at {}", input_time.format("%H:%M %p"))
    } else if input_time.day() == now.day() - 1 {
        format!("Yesterday at {}", input_time.format("%H:%M %p"))
    } else {
        format!(
            "{}, {}",
            input_time.format("%d/%m/%y"),
            input_time.format("%H:%M %p")
        )
    }
}
