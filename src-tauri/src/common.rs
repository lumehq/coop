use nostr_sdk::prelude::*;

pub fn is_target(target: &PublicKey, tags: &Vec<Tag>) -> bool {
	for tag in tags {
		if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
			if public_key == target {
				return true;
			}
		}
	}
	false
}
