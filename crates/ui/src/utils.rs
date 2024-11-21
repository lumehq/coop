use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;
use std::collections::HashSet;

pub fn get_all_accounts_from_keyring() -> HashSet<PublicKey> {
    let search = Search::new().expect("Keyring not working.");
    let results = search.by_service("Coop Safe Storage");
    let list = List::list_credentials(&results, Limit::All);
    let accounts: HashSet<PublicKey> = list
        .split_whitespace()
        .filter(|v| v.starts_with("npub1") && !v.ends_with("coop"))
        .filter_map(|i| PublicKey::from_bech32(i).ok())
        .collect();

    accounts
}
