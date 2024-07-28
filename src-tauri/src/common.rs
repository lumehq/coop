use nostr_sdk::prelude::*;

pub fn is_member(groups: Vec<&PublicKey>, tags: &Vec<Tag>) -> bool {
	for tag in tags {
		if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
			if groups.contains(&public_key) {
				return true;
			}
		}
	}
	false
}
