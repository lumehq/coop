use common::{
    last_seen::LastSeen,
    profile::NostrProfile,
    utils::{random_name, room_hash},
};
use gpui::{App, AppContext, Entity, SharedString};
use nostr_sdk::prelude::*;
use state::get_client;
use std::collections::HashSet;

pub struct Room {
    pub id: u64,
    pub last_seen: LastSeen,
    /// Subject of the room (Nostr)
    pub title: Option<SharedString>,
    /// Display name of the room (used for display purposes in Coop)
    pub display_name: Entity<Option<SharedString>>,
    /// All members of the room
    pub members: Entity<Vec<NostrProfile>>,
    /// Store all new messages
    pub new_messages: Entity<Vec<Event>>,
}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Room {
    pub fn new(event: &Event, cx: &mut App) -> Self {
        let id = room_hash(event);
        let last_seen = LastSeen(event.created_at);

        // Initialize display name model
        let display_name = cx.new(|_| None);
        let async_name = display_name.downgrade();

        // Initialize new messages model
        let new_messages = cx.new(|_| Vec::new());

        // Initialize members model
        let members = cx.new(|cx| {
            let members: Vec<NostrProfile> = vec![];
            let mut pubkeys = vec![];
            // Get all pubkeys from event's tags
            pubkeys.extend(event.tags.public_keys().collect::<HashSet<_>>());
            pubkeys.push(event.pubkey);

            let client = get_client();
            let (tx, rx) = oneshot::channel::<Vec<NostrProfile>>();

            cx.background_spawn(async move {
                let mut profiles = Vec::new();

                for public_key in pubkeys.into_iter() {
                    if let Ok(metadata) = client.database().metadata(public_key).await {
                        profiles.push(NostrProfile::new(public_key, metadata.unwrap_or_default()));
                    }
                }

                _ = tx.send(profiles);
            })
            .detach();

            cx.spawn(|this, cx| async move {
                if let Ok(profiles) = rx.await {
                    _ = cx.update(|cx| {
                        if profiles.len() > 2 {
                            let merged = profiles
                                .iter()
                                .take(2)
                                .map(|profile| profile.name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ");

                            let name: SharedString =
                                format!("{}, +{}", merged, profiles.len() - 2).into();

                            _ = async_name.update(cx, |this, cx| {
                                *this = Some(name);
                                cx.notify();
                            })
                        }

                        _ = this.update(cx, |this: &mut Vec<NostrProfile>, cx| {
                            this.extend(profiles);
                            cx.notify();
                        });
                    });
                }
            })
            .detach();

            members
        });

        // Get title from event's tags, create a random title if not found
        let title = if let Some(tag) = event.tags.find(TagKind::Subject) {
            tag.content().map(|s| s.to_owned().into())
        } else {
            Some(random_name(2).into())
        };

        Self {
            id,
            last_seen,
            title,
            display_name,
            members,
            new_messages,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get room's member by public key
    pub fn member(&self, public_key: &PublicKey, cx: &App) -> Option<NostrProfile> {
        self.members
            .read(cx)
            .iter()
            .find(|m| &m.public_key() == public_key)
            .cloned()
    }

    /// Get room's display name
    pub fn name(&self, cx: &App) -> Option<SharedString> {
        self.display_name.read(cx).clone()
    }

    /// Determine if room is a group
    pub fn is_group(&self, cx: &App) -> bool {
        self.members.read(cx).len() > 2
    }

    /// Get room's last seen
    pub fn last_seen(&self) -> &LastSeen {
        &self.last_seen
    }
}
