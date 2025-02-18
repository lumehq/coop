use chrono::{Datelike, Local, TimeZone};
use gpui::SharedString;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LastSeen(pub Timestamp);

impl LastSeen {
    pub fn ago(&self) -> SharedString {
        let now = Local::now();
        let input_time = Local.timestamp_opt(self.0.as_u64() as i64, 0).unwrap();
        let diff = (now - input_time).num_hours();

        if diff < 24 {
            let duration = now.signed_duration_since(input_time);

            if duration.num_seconds() < 60 {
                "now".to_string().into()
            } else if duration.num_minutes() == 1 {
                "1m".to_string().into()
            } else if duration.num_minutes() < 60 {
                format!("{}m", duration.num_minutes()).into()
            } else if duration.num_hours() == 1 {
                "1h".to_string().into()
            } else if duration.num_hours() < 24 {
                format!("{}h", duration.num_hours()).into()
            } else if duration.num_days() == 1 {
                "1d".to_string().into()
            } else {
                format!("{}d", duration.num_days()).into()
            }
        } else {
            input_time.format("%b %d").to_string().into()
        }
    }

    pub fn human_readable(&self) -> SharedString {
        let now = Local::now();
        let input_time = Local.timestamp_opt(self.0.as_u64() as i64, 0).unwrap();

        if input_time.day() == now.day() {
            format!("Today at {}", input_time.format("%H:%M %p")).into()
        } else if input_time.day() == now.day() - 1 {
            format!("Yesterday at {}", input_time.format("%H:%M %p")).into()
        } else {
            format!(
                "{}, {}",
                input_time.format("%d/%m/%y"),
                input_time.format("%H:%M %p")
            )
            .into()
        }
    }

    pub fn set(&mut self, created_at: Timestamp) {
        self.0 = created_at
    }
}
