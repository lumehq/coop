use std::path::PathBuf;

use dirs::config_dir;
use qrcode_generator::QrCodeEcc;

pub fn create_qr(data: &str) -> Result<PathBuf, anyhow::Error> {
    let config_dir = config_dir().expect("Config directory not found");
    let path = config_dir.join("Coop/nostr_connect.png");

    qrcode_generator::to_png_to_file(data, QrCodeEcc::Low, 512, &path)?;

    Ok(path)
}
