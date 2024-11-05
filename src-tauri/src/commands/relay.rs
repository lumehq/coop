use nostr_sdk::prelude::*;
use std::{
    fs::OpenOptions,
    io::{self, BufRead, Write},
    time::Duration,
};
use tauri::{Manager, State};

use crate::Nostr;

#[tauri::command]
#[specta::specta]
pub fn get_bootstrap_relays(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let relays_path = app
        .path()
        .resolve("resources/relays.txt", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;

    let file = std::fs::File::open(relays_path).map_err(|e| e.to_string())?;
    let reader = io::BufReader::new(file);

    reader
        .lines()
        .collect::<Result<Vec<String>, io::Error>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_bootstrap_relays(relays: String, app: tauri::AppHandle) -> Result<(), String> {
    let relays_path = app
        .path()
        .resolve("resources/relays.txt", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .open(relays_path)
        .map_err(|e| e.to_string())?;

    file.write_all(relays.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_inbox_relays(
    user_id: String,
    state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
    let client = &state.client;
    let public_key = PublicKey::parse(user_id).map_err(|e| e.to_string())?;
    let filter = Filter::new()
        .kind(Kind::Custom(10050))
        .author(public_key)
        .limit(1);

    let mut events = Events::new(&[filter.clone()]);

    let mut rx = client
        .stream_events(vec![filter], Some(Duration::from_secs(3)))
        .await
        .map_err(|e| e.to_string())?;

    while let Some(event) = rx.next().await {
        events.insert(event);
    }

    if let Some(event) = events.first() {
        let urls = event
            .tags
            .iter()
            .filter_map(|tag| {
                if let Some(TagStandard::Relay(relay)) = tag.as_standardized() {
                    Some(relay.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(urls)
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn ensure_inbox_relays(
    user_id: String,
    state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
    let public_key = PublicKey::parse(user_id).map_err(|e| e.to_string())?;
    let relays = state.inbox_relays.read().await;

    match relays.get(&public_key) {
        Some(relays) => {
            if relays.is_empty() {
                Err("404".into())
            } else {
                Ok(relays.to_owned())
            }
        }
        None => Err("404".into()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn set_inbox_relays(relays: Vec<String>, state: State<'_, Nostr>) -> Result<(), String> {
    let client = &state.client;

    let tags = relays
        .into_iter()
        .map(|t| Tag::custom(TagKind::Relay, vec![t]))
        .collect::<Vec<_>>();
    let event = EventBuilder::new(Kind::Custom(10050), "", tags);

    match client.send_event_builder(event).await {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn connect_inbox_relays(
    user_id: String,
    ignore_cache: bool,
    state: State<'_, Nostr>,
) -> Result<Vec<String>, String> {
    let client = &state.client;
    let public_key = PublicKey::parse(&user_id).map_err(|e| e.to_string())?;

    let mut inbox_relays = state.inbox_relays.write().await;

    if !ignore_cache {
        if let Some(relays) = inbox_relays.get(&public_key) {
            for url in relays {
                if let Ok(relay) = client.relay(url).await {
                    if !relay.is_connected() {
                        if let Err(e) = client.connect_relay(url).await {
                            println!("Connect relay failed: {}", e)
                        }
                    }
                } else if let Err(e) = client.add_relay(url).await {
                    println!("Connect relay failed: {}", e)
                }
            }
            return Ok(relays.to_owned());
        };
    };

    let filter = Filter::new()
        .kind(Kind::Custom(10050))
        .author(public_key)
        .limit(1);

    let mut events = Events::new(&[filter.clone()]);

    let mut rx = client
        .stream_events(vec![filter], Some(Duration::from_secs(3)))
        .await
        .map_err(|e| e.to_string())?;

    while let Some(event) = rx.next().await {
        events.insert(event);
    }

    if let Some(event) = events.first() {
        let mut relays = Vec::new();

        for tag in event.tags.iter() {
            if let Some(TagStandard::Relay(relay)) = tag.as_standardized() {
                let url = relay.to_string();
                let _ = client.add_relay(&url).await;
                let _ = client.connect_relay(&url).await;

                // Workaround for https://github.com/rust-nostr/nostr/issues/509
                // TODO: remove
                let filter = Filter::new().kind(Kind::TextNote).limit(0);
                let _ = client
                    .fetch_events_from(
                        vec![url.clone()],
                        vec![filter],
                        Some(Duration::from_secs(3)),
                    )
                    .await;

                relays.push(url)
            }
        }

        // Update state
        inbox_relays.insert(public_key, relays.clone());

        Ok(relays)
    } else {
        Err("User's inbox relays not found.".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect_inbox_relays(
    user_id: String,
    state: State<'_, Nostr>,
) -> Result<(), String> {
    let client = &state.client;
    let public_key = PublicKey::parse(&user_id).map_err(|e| e.to_string())?;
    let inbox_relays = state.inbox_relays.read().await;

    if let Some(relays) = inbox_relays.get(&public_key) {
        for relay in relays {
            let _ = client.disconnect_relay(relay).await;
        }
    }

    Ok(())
}
