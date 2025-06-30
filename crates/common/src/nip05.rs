use anyhow::anyhow;
use nostr::prelude::*;
use reqwest::Client as ReqClient;

pub async fn nip05_verify(public_key: PublicKey, address: &str) -> Result<bool, anyhow::Error> {
    let req_client = ReqClient::new();
    let address = Nip05Address::parse(address)?;

    // Get NIP-05 response
    let res = req_client.get(address.url().to_string()).send().await?;
    let json: Value = res.json().await?;

    let verify = nip05::verify_from_json(&public_key, &address, &json);

    Ok(verify)
}

pub async fn nip05_profile(address: &str) -> Result<Nip05Profile, anyhow::Error> {
    let req_client = ReqClient::new();
    let address = Nip05Address::parse(address)?;

    // Get NIP-05 response
    let res = req_client.get(address.url().to_string()).send().await?;
    let json: Value = res.json().await?;

    if let Ok(profile) = Nip05Profile::from_json(&address, &json) {
        Ok(profile)
    } else {
        Err(anyhow!("Failed to get NIP-05 profile"))
    }
}
