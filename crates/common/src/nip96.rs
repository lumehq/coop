use anyhow::anyhow;
use nostr::hashes::sha256::Hash as Sha256Hash;
use nostr::hashes::Hash;
use nostr::prelude::*;
use nostr_sdk::prelude::*;
use reqwest::{multipart, Client as ReqClient, Response};

pub(crate) fn make_multipart_form(
    file_data: Vec<u8>,
    mime_type: Option<&str>,
) -> Result<multipart::Form, anyhow::Error> {
    let form_file_part = multipart::Part::bytes(file_data).file_name("filename");

    // Set the part's MIME type, or leave it as is if mime_type is None

    let part = match mime_type {
        Some(mime) => form_file_part.mime_str(mime)?,
        None => form_file_part,
    };

    Ok(multipart::Form::new().part("file", part))
}

pub(crate) async fn upload<T>(
    signer: &T,
    desc: &ServerConfig,
    file_data: Vec<u8>,
    mime_type: Option<&str>,
) -> Result<Url, anyhow::Error>
where
    T: NostrSigner,
{
    let payload: Sha256Hash = Sha256Hash::hash(&file_data);
    let data: HttpData = HttpData::new(desc.api_url.clone(), HttpMethod::POST).payload(payload);
    let nip98_auth: String = data.to_authorization(signer).await?;

    // Make form
    let form: multipart::Form = make_multipart_form(file_data, mime_type)?;

    // Make req client
    let req_client = ReqClient::new();

    // Send
    let response: Response = req_client
        .post(desc.api_url.clone())
        .header("Authorization", nip98_auth)
        .multipart(form)
        .send()
        .await?;

    // Parse response
    let json: Value = response.json().await?;
    let upload_response = nip96::UploadResponse::from_json(json.to_string())?;

    if upload_response.status == UploadResponseStatus::Error {
        return Err(anyhow!(upload_response.message));
    }

    Ok(upload_response.download_url()?.to_owned())
}

pub async fn nip96_upload(
    client: &Client,
    server: &Url,
    file: Vec<u8>,
) -> Result<Url, anyhow::Error> {
    let req_client = ReqClient::new();
    let config_url = nip96::get_server_config_url(server)?;

    // Get
    let res = req_client.get(config_url.to_string()).send().await?;
    let json: Value = res.json().await?;

    let config = nip96::ServerConfig::from_json(json.to_string())?;
    let signer = client.signer().await?;

    let url = upload(&signer, &config, file, None).await?;

    Ok(url)
}
