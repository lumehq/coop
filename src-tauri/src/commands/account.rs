use keyring::Entry;
use keyring_search::{Limit, List, Search};
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{collections::HashSet, str::FromStr, time::Duration};
use tauri::{Manager, State};

use crate::{Nostr, SUBSCRIPTION_ID};

#[derive(Clone, Serialize)]
pub struct EventPayload {
    event: String, // JSON String
    sender: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
struct Account {
    password: String,
    nostr_connect: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_metadata(id: String, state: State<'_, Nostr>) -> Result<String, String> {
    let client = &state.client;
    let public_key = PublicKey::parse(&id).map_err(|e| e.to_string())?;

    let filter = Filter::new()
        .author(public_key)
        .kind(Kind::Metadata)
        .limit(1);

    let events = client
        .database()
        .query(vec![filter])
        .await
        .map_err(|e| e.to_string())?;

    match events.first() {
        Some(event) => match Metadata::from_json(&event.content) {
            Ok(metadata) => Ok(metadata.as_json()),
            Err(e) => Err(e.to_string()),
        },
        None => Err("Metadata not found".into()),
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_accounts() -> Vec<String> {
    let search = Search::new().expect("Unexpected.");
    let results = search.by_service("Coop Secret Storage");
    let list = List::list_credentials(&results, Limit::All);
    let accounts: HashSet<String> = list
        .split_whitespace()
        .filter(|v| v.starts_with("npub1"))
        .map(String::from)
        .collect();

    accounts.into_iter().collect()
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_account(state: State<'_, Nostr>) -> Result<String, String> {
    let client = &state.client;
    let signer = client.signer().await.map_err(|e| e.to_string())?;
    let public_key = signer.get_public_key().await.map_err(|e| e.to_string())?;
    let bech32 = public_key.to_bech32().map_err(|e| e.to_string())?;

    Ok(bech32)
}

#[tauri::command]
#[specta::specta]
pub async fn create_account(
    name: String,
    about: String,
    picture: String,
    password: String,
    state: State<'_, Nostr>,
) -> Result<String, String> {
    let client = &state.client;
    let keys = Keys::generate();
    let npub = keys.public_key().to_bech32().map_err(|e| e.to_string())?;
    let secret_key = keys.secret_key();
    let enc = EncryptedSecretKey::new(secret_key, password, 16, KeySecurity::Medium)
        .map_err(|err| err.to_string())?;
    let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

    // Save account
    let keyring = Entry::new("Coop Secret Storage", &npub).map_err(|e| e.to_string())?;
    let account = Account {
        password: enc_bech32,
        nostr_connect: None,
    };
    let j = serde_json::to_string(&account).map_err(|e| e.to_string())?;
    let _ = keyring.set_password(&j);

    // Update signer
    client.set_signer(keys).await;

    let mut metadata = Metadata::new()
        .display_name(name.clone())
        .name(name.to_lowercase())
        .about(about);

    if let Ok(url) = Url::parse(&picture) {
        metadata = metadata.picture(url)
    }

    match client.set_metadata(&metadata).await {
        Ok(_) => Ok(npub),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn import_account(key: String, password: String) -> Result<String, String> {
    let (npub, enc_bech32) = match key.starts_with("ncryptsec") {
        true => {
            let enc = EncryptedSecretKey::from_bech32(key).map_err(|err| err.to_string())?;
            let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;
            let secret_key = enc.to_secret_key(password).map_err(|err| err.to_string())?;
            let keys = Keys::new(secret_key);
            let npub = keys.public_key().to_bech32().unwrap();

            (npub, enc_bech32)
        }
        false => {
            let secret_key = SecretKey::from_bech32(key).map_err(|err| err.to_string())?;
            let keys = Keys::new(secret_key.clone());
            let npub = keys.public_key().to_bech32().unwrap();

            let enc = EncryptedSecretKey::new(&secret_key, password, 16, KeySecurity::Medium)
                .map_err(|err| err.to_string())?;

            let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

            (npub, enc_bech32)
        }
    };

    let keyring = Entry::new("Coop Secret Storage", &npub).map_err(|e| e.to_string())?;

    let account = Account {
        password: enc_bech32,
        nostr_connect: None,
    };

    let pwd = serde_json::to_string(&account).map_err(|e| e.to_string())?;
    keyring.set_password(&pwd).map_err(|e| e.to_string())?;

    Ok(npub)
}

#[tauri::command]
#[specta::specta]
pub async fn connect_account(uri: String, state: State<'_, Nostr>) -> Result<String, String> {
    let client = &state.client;

    match NostrConnectURI::parse(uri.clone()) {
        Ok(bunker_uri) => {
            // Local user
            let app_keys = Keys::generate();
            let app_secret = app_keys.secret_key().to_secret_hex();

            // Get remote user
            let remote_user = bunker_uri.remote_signer_public_key().unwrap();
            let remote_npub = remote_user.to_bech32().unwrap();

            match NostrConnect::new(bunker_uri, app_keys, Duration::from_secs(120), None) {
                Ok(signer) => {
                    let mut url = Url::parse(&uri).unwrap();
                    let query: Vec<(String, String)> = url
                        .query_pairs()
                        .filter(|(name, _)| name != "secret")
                        .map(|(name, value)| (name.into_owned(), value.into_owned()))
                        .collect();
                    url.query_pairs_mut().clear().extend_pairs(&query);

                    let key = format!("{}_nostrconnect", remote_npub);
                    let keyring = Entry::new("Coop Secret Storage", &key).unwrap();
                    let account = Account {
                        password: app_secret,
                        nostr_connect: Some(url.to_string()),
                    };
                    let j = serde_json::to_string(&account).map_err(|e| e.to_string())?;
                    let _ = keyring.set_password(&j);

                    // Update signer
                    let _ = client.set_signer(signer).await;

                    Ok(remote_npub)
                }
                Err(err) => Err(err.to_string()),
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn reset_password(key: String, password: String) -> Result<(), String> {
    let secret_key = SecretKey::from_bech32(key).map_err(|err| err.to_string())?;
    let keys = Keys::new(secret_key.clone());
    let npub = keys.public_key().to_bech32().unwrap();

    let enc = EncryptedSecretKey::new(&secret_key, password, 16, KeySecurity::Medium)
        .map_err(|err| err.to_string())?;
    let enc_bech32 = enc.to_bech32().map_err(|err| err.to_string())?;

    let keyring = Entry::new("Coop Secret Storage", &npub).map_err(|e| e.to_string())?;
    let account = Account {
        password: enc_bech32,
        nostr_connect: None,
    };
    let j = serde_json::to_string(&account).map_err(|e| e.to_string())?;
    let _ = keyring.set_password(&j);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn delete_account(id: String) -> Result<(), String> {
    let keyring = Entry::new("Coop Secret Storage", &id).map_err(|e| e.to_string())?;
    let _ = keyring.delete_credential();

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_contact_list(state: State<'_, Nostr>) -> Result<Vec<String>, String> {
    let client = &state.client;

    match client.get_contact_list(Some(Duration::from_secs(10))).await {
        Ok(contacts) => {
            let list = contacts
                .into_iter()
                .map(|c| c.public_key.to_hex())
                .collect::<Vec<_>>();
            Ok(list)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn login(
    account: String,
    password: String,
    state: State<'_, Nostr>,
    handle: tauri::AppHandle,
) -> Result<String, String> {
    let client = &state.client;
    let keyring = Entry::new("Coop Secret Storage", &account).map_err(|e| e.to_string())?;

    let account = match keyring.get_password() {
        Ok(pw) => {
            let account: Account = serde_json::from_str(&pw).map_err(|e| e.to_string())?;
            account
        }
        Err(e) => return Err(e.to_string()),
    };

    let public_key = match account.nostr_connect {
        None => {
            let ncryptsec =
                EncryptedSecretKey::from_bech32(account.password).map_err(|e| e.to_string())?;
            let secret_key = ncryptsec
                .to_secret_key(password)
                .map_err(|_| "Wrong password.")?;
            let keys = Keys::new(secret_key);
            let public_key = keys.public_key();

            // Update signer
            client.set_signer(keys).await;

            public_key
        }
        Some(bunker) => {
            let uri = NostrConnectURI::parse(bunker).map_err(|e| e.to_string())?;
            let public_key = uri.remote_signer_public_key().unwrap().to_owned();
            let app_keys = Keys::from_str(&account.password).map_err(|e| e.to_string())?;

            match NostrConnect::new(uri, app_keys, Duration::from_secs(120), None) {
                Ok(signer) => {
                    // Update signer
                    client.set_signer(signer).await;
                    // Return public key
                    public_key
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    };

    let filter = Filter::new()
        .kind(Kind::Custom(10050))
        .author(public_key)
        .limit(1);

    let mut rx = client
        .stream_events(vec![filter], Some(Duration::from_secs(3)))
        .await
        .map_err(|e| e.to_string())?;

    while let Some(event) = rx.next().await {
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

        for url in urls.iter() {
            let _ = client.add_relay(url).await;
            let _ = client.connect_relay(url).await;
        }

        let mut inbox_relays = state.inbox_relays.write().await;
        inbox_relays.insert(public_key, urls);
    }

    tauri::async_runtime::spawn(async move {
        let state = handle.state::<Nostr>();
        let client = &state.client;

        let inbox_relays = state.inbox_relays.read().await;
        let relays = inbox_relays.get(&public_key).unwrap().to_owned();

        let sub_id = SubscriptionId::new(SUBSCRIPTION_ID);
        let new_message = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(public_key)
            .limit(0);

        if let Err(e) = client
            .subscribe_with_id_to(&relays, sub_id, vec![new_message], None)
            .await
        {
            println!("Subscribe error: {}", e)
        };

        let filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(public_key)
            .limit(200);

        let mut rx = client
            .stream_events_from(&relays, vec![filter], Some(Duration::from_secs(40)))
            .await
            .unwrap();

        while let Some(event) = rx.next().await {
            println!("Event: {}", event.as_json());
        }

        // handle.emit("synchronized", ()).unwrap();
    });

    Ok(public_key.to_hex())
}
