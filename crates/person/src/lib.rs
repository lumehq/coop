use std::collections::{HashMap, HashSet};

use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;

pub fn init(cx: &mut App) {
    PersonRegistry::set_global(cx.new(PersonRegistry::new), cx);
}

struct GlobalPersonRegistry(Entity<PersonRegistry>);

impl Global for GlobalPersonRegistry {}

/// Person Registry
#[derive(Debug)]
pub struct PersonRegistry {
    /// Collection of all persons (user profiles)
    persons: HashMap<PublicKey, Entity<Profile>>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 3]>,
}

impl PersonRegistry {
    /// Retrieve the global person registry
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalPersonRegistry>().0.clone()
    }

    /// Set the global person registry instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalPersonRegistry(state));
    }

    /// Create a new person registry instance
    fn new(cx: &mut Context<Self>) -> Self {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Channel for communication between nostr and gpui
        let (tx, rx) = flume::bounded::<Profile>(100);

        let mut tasks = smallvec![];

        tasks.push(
            // Handle nostr notifications
            cx.background_spawn({
                let client = client.clone();

                async move { Self::handle_notifications(&client, &tx).await }
            }),
        );

        tasks.push(
            // Update GPUI state
            cx.spawn(async move |this, cx| {
                while let Ok(profile) = rx.recv_async().await {
                    this.update(cx, |this, cx| {
                        this.insert(profile, cx);
                    })
                    .ok();
                }
            }),
        );

        tasks.push(
            // Load all user profiles from the database
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .await_on_background(async move { Self::load_persons(&client).await })
                    .await;

                match result {
                    Ok(profiles) => {
                        this.update(cx, |this, cx| {
                            this.bulk_inserts(profiles, cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        log::error!("Failed to load all persons from the database: {e}");
                    }
                };
            }),
        );

        Self {
            persons: HashMap::new(),
            _tasks: tasks,
        }
    }

    /// Handle nostr notifications
    async fn handle_notifications(client: &Client, tx: &flume::Sender<Profile>) {
        let mut notifications = client.notifications();
        let mut processed_events = HashSet::new();

        while let Ok(notification) = notifications.recv().await {
            let RelayPoolNotification::Message { message, .. } = notification else {
                // Skip if the notification is not a message
                continue;
            };

            if let RelayMessage::Event { event, .. } = message {
                if !processed_events.insert(event.id) {
                    // Skip if the event has already been processed
                    continue;
                }

                // Only process metadata events
                if event.kind == Kind::Metadata {
                    let metadata = Metadata::from_json(&event.content).unwrap_or_default();
                    let profile = Profile::new(event.pubkey, metadata);

                    tx.send_async(profile).await.ok();
                };
            }
        }
    }

    /// Load all user profiles from the database
    async fn load_persons(client: &Client) -> Result<Vec<Profile>, Error> {
        let filter = Filter::new().kind(Kind::Metadata).limit(200);
        let events = client.database().query(filter).await?;

        let mut profiles = vec![];

        for event in events.into_iter() {
            let metadata = Metadata::from_json(event.content).unwrap_or_default();
            let profile = Profile::new(event.pubkey, metadata);
            profiles.push(profile);
        }

        Ok(profiles)
    }

    /// Insert batch of persons
    fn bulk_inserts(&mut self, profiles: Vec<Profile>, cx: &mut Context<Self>) {
        for profile in profiles.into_iter() {
            self.persons
                .insert(profile.public_key(), cx.new(|_| profile));
        }
        cx.notify();
    }

    /// Insert or update a person
    pub fn insert(&mut self, profile: Profile, cx: &mut App) {
        let public_key = profile.public_key();

        match self.persons.get(&public_key) {
            Some(person) => {
                person.update(cx, |this, cx| {
                    *this = profile;
                    cx.notify();
                });
            }
            None => {
                self.persons.insert(public_key, cx.new(|_| profile));
            }
        }
    }

    /// Get single person by public key
    pub fn get(&self, public_key: &PublicKey, cx: &App) -> Profile {
        self.persons
            .get(public_key)
            .map(|e| e.read(cx))
            .cloned()
            .unwrap_or(Profile::new(public_key.to_owned(), Metadata::default()))
    }
}
