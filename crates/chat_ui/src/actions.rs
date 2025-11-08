use encryption::SignerKind;
use gpui::Action;
use nostr_sdk::prelude::*;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = chat, no_json)]
pub struct SeenOn(pub EventId);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = chat, no_json)]
pub struct SetSigner(pub SignerKind);

/// Define a open public key action
#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = pubkey, no_json)]
pub struct OpenPublicKey(pub PublicKey);

/// Define a copy inline public key action
#[derive(Action, Clone, PartialEq, Eq, Deserialize, Debug)]
#[action(namespace = pubkey, no_json)]
pub struct CopyPublicKey(pub PublicKey);
