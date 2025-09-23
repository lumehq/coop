use std::sync::Arc;

use anyhow::{anyhow, Error};
use chrono::{Local, TimeZone};
use global::constants::IMAGE_RESIZE_SERVICE;
use gpui::{Image, ImageFormat, SharedString, SharedUri};
use nostr_sdk::prelude::*;
use qrcode::render::svg;
use qrcode::QrCode;

const NOW: &str = "now";
const SECONDS_IN_MINUTE: i64 = 60;
const MINUTES_IN_HOUR: i64 = 60;
const HOURS_IN_DAY: i64 = 24;
const DAYS_IN_MONTH: i64 = 30;
const FALLBACK_IMG: &str = "https://image.nostr.build/c30703b48f511c293a9003be8100cdad37b8798b77a1dc3ec6eb8a20443d5dea.png";

pub trait RenderedProfile {
    fn avatar(&self, proxy: bool) -> SharedUri;
    fn display_name(&self) -> SharedString;
}

impl RenderedProfile for Profile {
    fn avatar(&self, proxy: bool) -> SharedUri {
        self.metadata()
            .picture
            .as_ref()
            .filter(|picture| !picture.is_empty())
            .map(|picture| {
                if proxy {
                    let url = format!(
                        "{IMAGE_RESIZE_SERVICE}/?url={picture}&w=100&h=100&fit=cover&mask=circle&default={FALLBACK_IMG}&n=-1"
                    );

                    SharedUri::from(url)
                } else {
                    SharedUri::from(picture)
                }
            })
            .unwrap_or_else(|| SharedUri::from("brand/avatar.png"))
    }

    fn display_name(&self) -> SharedString {
        if let Some(display_name) = self.metadata().display_name.as_ref() {
            if !display_name.is_empty() {
                return SharedString::from(display_name);
            }
        }

        if let Some(name) = self.metadata().name.as_ref() {
            if !name.is_empty() {
                return SharedString::from(name);
            }
        }

        SharedString::from(shorten_pubkey(self.public_key(), 4))
    }
}

pub trait ReadableTimestamp {
    fn to_human_time(&self) -> String;
    fn to_ago(&self) -> String;
}

impl ReadableTimestamp for Timestamp {
    fn to_human_time(&self) -> String {
        let input_time = match Local.timestamp_opt(self.as_u64() as i64, 0) {
            chrono::LocalResult::Single(time) => time,
            _ => return "9999".into(),
        };

        let now = Local::now();
        let input_date = input_time.date_naive();
        let now_date = now.date_naive();
        let yesterday_date = (now - chrono::Duration::days(1)).date_naive();

        let time_format = input_time.format("%H:%M %p");

        match input_date {
            date if date == now_date => format!("Today at {time_format}"),
            date if date == yesterday_date => format!("Yesterday at {time_format}"),
            _ => format!("{}, {time_format}", input_time.format("%d/%m/%y")),
        }
    }

    fn to_ago(&self) -> String {
        let input_time = match Local.timestamp_opt(self.as_u64() as i64, 0) {
            chrono::LocalResult::Single(time) => time,
            _ => return "1m".into(),
        };

        let now = Local::now();
        let duration = now.signed_duration_since(input_time);

        match duration {
            d if d.num_seconds() < SECONDS_IN_MINUTE => NOW.into(),
            d if d.num_minutes() < MINUTES_IN_HOUR => format!("{}m", d.num_minutes()),
            d if d.num_hours() < HOURS_IN_DAY => format!("{}h", d.num_hours()),
            d if d.num_days() < DAYS_IN_MONTH => format!("{}d", d.num_days()),
            _ => input_time.format("%b %d").to_string(),
        }
    }
}

pub trait TextUtils {
    fn to_public_key(&self) -> Result<PublicKey, Error>;
    fn to_qr(&self) -> Option<Arc<Image>>;
}

impl<T: AsRef<str>> TextUtils for T {
    fn to_public_key(&self) -> Result<PublicKey, Error> {
        let s = self.as_ref();
        if s.starts_with("nprofile1") {
            Ok(Nip19Profile::from_bech32(s)?.public_key)
        } else if s.starts_with("npub1") {
            Ok(PublicKey::parse(s)?)
        } else {
            Err(anyhow!("Invalid public key"))
        }
    }

    fn to_qr(&self) -> Option<Arc<Image>> {
        let s = self.as_ref();
        let code = QrCode::new(s).unwrap();
        let svg = code
            .render()
            .min_dimensions(256, 256)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#FFFFFF"))
            .build();

        Some(Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            svg.into_bytes(),
        )))
    }
}

pub fn shorten_pubkey(public_key: PublicKey, len: usize) -> String {
    let Ok(pubkey) = public_key.to_bech32();

    format!(
        "{}:{}",
        &pubkey[0..(len + 1)],
        &pubkey[pubkey.len() - len..]
    )
}
