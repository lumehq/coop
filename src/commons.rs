use std::collections::HashSet;

use keyring_search::{Limit, List, Search};

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
