use std::collections::HashSet;

use chrono::{DateTime, Duration};
use keyring_search::{Limit, List, Search};
use nostr_sdk::Timestamp;

pub fn get_accounts() -> Vec<String> {
	let search = Search::new().expect("Secure Storage is not working.");
	let results = search.by_user("nostr_secret");
	let list = List::list_credentials(&results, Limit::All);
	let accounts: HashSet<String> = list
		.split_whitespace()
		.filter(|v| v.starts_with("npub1"))
		.map(String::from)
		.collect();

	accounts.into_iter().collect()
}

pub fn time_ago(time: Timestamp) -> String {
	let t_now = Timestamp::now().as_u64();
	let t_input = time.as_u64();

	let now = DateTime::from_timestamp(t_now as i64, 0).unwrap();
	let input = DateTime::from_timestamp(t_input as i64, 0).unwrap();

	let diff = now - input;

	if diff < Duration::hours(24) {
		if diff < Duration::seconds(60) {
			return " now".to_string();
		} else if diff < Duration::minutes(60) {
			return format!("{}m", diff.num_minutes());
		} else if diff < Duration::hours(24) {
			return format!("{}h", diff.num_hours());
		}
	}

	format!("{}d", diff.num_days())
}
