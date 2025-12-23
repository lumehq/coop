use encryption::SignerKind;
use gpui::Action;
use nostr_sdk::prelude::*;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = room, no_json)]
pub enum RoomEvent {
    View(PublicKey),
    Copy(PublicKey),
    Relay(EventId),
    SetEmoji(String),
    SetSigner(SignerKind),
    SetSubject,
    Refresh,
}
