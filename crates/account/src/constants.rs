use std::{env, sync::OnceLock};

pub const USER_KEYRING: &str = "Coop User Storage";
pub const DEVICE_KEYRING: &str = "Coop Device Storage";

pub const DEVICE_ANNOUNCEMENT_KIND: u16 = 10044;
pub const DEVICE_REQUEST_KIND: u16 = 4454;
pub const DEVICE_RESPONSE_KIND: u16 = 4455;

static APP_NAME: OnceLock<String> = OnceLock::new();

pub fn get_app_name() -> &'static str {
    APP_NAME.get_or_init(|| format!("Coop on {}", env::consts::OS.to_string().to_uppercase()))
}
