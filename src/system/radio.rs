use dioxus_radio::hooks::RadioChannel;
use nostr_sdk::prelude::*;

#[derive(Default)]
pub struct Data {
	pub current_user: String,
	pub current_chat: String,
	pub chats: Vec<UnsignedEvent>,
	pub incoming: Vec<UnsignedEvent>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum DataChannel {
	AllChat,
	NewChat,
	SetCurrentUser,
	SetCurrentChat,
	NewIncoming,
}

impl RadioChannel<Data> for DataChannel {}