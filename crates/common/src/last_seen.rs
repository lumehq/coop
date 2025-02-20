use chrono::{Local, TimeZone};
use gpui::SharedString;
use nostr_sdk::prelude::*;

const NOW: &str = "now";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LastSeen(pub Timestamp);

impl LastSeen {
    pub fn ago(&self) -> SharedString {
        let now = Local::now();
        let input_time = match Local.timestamp_opt(self.0.as_u64() as i64, 0) {
            chrono::LocalResult::Single(time) => time,
            _ => return "Invalid timestamp".into(),
        };
        let duration = now.signed_duration_since(input_time);

        match duration {
            d if d.num_seconds() < 60 => NOW.into(),
            d if d.num_minutes() < 60 => format!("{}m", d.num_minutes()),
            d if d.num_hours() < 24 => format!("{}h", d.num_hours()),
            d if d.num_days() < 30 => format!("{}d", d.num_days()),
            _ => input_time.format("%b %d").to_string(),
        }
        .into()
    }

    pub fn human_readable(&self) -> SharedString {
        let now = Local::now();
        let input_time = match Local.timestamp_opt(self.0.as_u64() as i64, 0) {
            chrono::LocalResult::Single(time) => time,
            _ => return "Invalid timestamp".into(),
        };

        let input_date = input_time.date_naive();
        let now_date = now.date_naive();
        let yesterday_date = (now - chrono::Duration::days(1)).date_naive();

        let time_format = input_time.format("%H:%M %p");

        match input_date {
            date if date == now_date => format!("Today at {time_format}"),
            date if date == yesterday_date => format!("Yesterday at {time_format}"),
            _ => format!("{}, {time_format}", input_time.format("%d/%m/%y")),
        }
        .into()
    }

    pub fn set(&mut self, created_at: Timestamp) {
        self.0 = created_at
    }
}
