use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Error};
use common::{EventUtils, BOOTSTRAP_RELAYS};
use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
pub use person::*;
use smallvec::{smallvec, SmallVec};
use state::{Announcement, NostrRegistry, TIMEOUT};

mod person;

pub fn init(cx: &mut App) {
    PersonRegistry::set_global(cx.new(PersonRegistry::new), cx);
}

struct GlobalPersonRegistry(Entity<PersonRegistry>);

impl Global for GlobalPersonRegistry {}

#[derive(Debug, Clone)]
enum Dispatch {
    Person(Box<Person>),
    Announcement(Box<Event>),
}

/// Person Registry
#[derive(Debug)]
pub struct PersonRegistry {
    /// Collection of all persons (user profiles)
    persons: HashMap<PublicKey, Entity<Person>>,

    /// Set of public keys that have been seen
    seen: Rc<RefCell<HashSet<PublicKey>>>,

    /// Sender for requesting metadata
    sender: flume::Sender<PublicKey>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 4]>,
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
        let (tx, rx) = flume::bounded::<Dispatch>(100);
        let (mta_tx, mta_rx) = flume::bounded::<PublicKey>(100);

        let mut tasks = smallvec![];

        tasks.push(
            // Handle nostr notifications
            cx.background_spawn({
                let client = client.clone();

                async move {
                    Self::handle_notifications(&client, &tx).await;
                }
            }),
        );

        tasks.push(
            // Handle metadata requests
            cx.background_spawn({
                let client = client.clone();

                async move {
                    Self::handle_requests(&client, &mta_rx).await;
                }
            }),
        );

        tasks.push(
            // Update GPUI state
            cx.spawn(async move |this, cx| {
                while let Ok(event) = rx.recv_async().await {
                    this.update(cx, |this, cx| {
                        match event {
                            Dispatch::Person(person) => {
                                this.insert(*person, cx);
                            }
                            Dispatch::Announcement(event) => {
                                this.set_announcement(&event, cx);
                            }
                        };
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
                    Ok(persons) => {
                        this.update(cx, |this, cx| {
                            this.bulk_inserts(persons, cx);
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
            seen: Rc::new(RefCell::new(HashSet::new())),
            sender: mta_tx,
            _tasks: tasks,
        }
    }

    /// Handle nostr notifications
    async fn handle_notifications(client: &Client, tx: &flume::Sender<Dispatch>) {
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

                match event.kind {
                    Kind::Metadata => {
                        let metadata = Metadata::from_json(&event.content).unwrap_or_default();
                        let person = Person::new(event.pubkey, metadata);
                        let val = Box::new(person);

                        // Send
                        tx.send_async(Dispatch::Person(val)).await.ok();
                    }
                    Kind::Custom(10044) => {
                        let val = Box::new(event.into_owned());

                        // Send
                        tx.send_async(Dispatch::Announcement(val)).await.ok();
                    }
                    Kind::ContactList => {
                        let public_keys = event.extract_public_keys();

                        // Get metadata for all public keys
                        Self::get_metadata(client, public_keys).await.ok();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Handle request for metadata
    async fn handle_requests(client: &Client, rx: &flume::Receiver<PublicKey>) {
        let mut batch: HashSet<PublicKey> = HashSet::new();

        loop {
            match flume::Selector::new()
                .recv(rx, |result| result.ok())
                .wait_timeout(Duration::from_secs(2))
            {
                Ok(Some(public_key)) => {
                    log::info!("Received public key: {}", public_key);
                    batch.insert(public_key);
                    // Process the batch if it's full
                    if batch.len() >= 20 {
                        Self::get_metadata(client, std::mem::take(&mut batch))
                            .await
                            .ok();
                    }
                }
                _ => {
                    Self::get_metadata(client, std::mem::take(&mut batch))
                        .await
                        .ok();
                }
            }
        }
    }

    /// Get metadata for all public keys in a event
    async fn get_metadata<I>(client: &Client, public_keys: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = PublicKey>,
    {
        let authors: Vec<PublicKey> = public_keys.into_iter().collect();
        let limit = authors.len();

        if authors.is_empty() {
            return Err(anyhow!("You need at least one public key"));
        }

        // Construct the subscription option
        let opts = SubscribeAutoCloseOptions::default()
            .exit_policy(ReqExitPolicy::ExitOnEOSE)
            .timeout(Some(Duration::from_secs(TIMEOUT)));

        // Construct the filter for metadata
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(authors)
            .limit(limit);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    /// Load all user profiles from the database
    async fn load_persons(client: &Client) -> Result<Vec<Person>, Error> {
        let filter = Filter::new().kind(Kind::Metadata).limit(200);
        let events = client.database().query(filter).await?;

        let mut persons = vec![];

        for event in events.into_iter() {
            let metadata = Metadata::from_json(event.content).unwrap_or_default();
            let person = Person::new(event.pubkey, metadata);
            persons.push(person);
        }

        Ok(persons)
    }

    /// Set profile encryption keys announcement
    fn set_announcement(&mut self, event: &Event, cx: &mut App) {
        if let Some(person) = self.persons.get(&event.pubkey) {
            let announcement = Announcement::from(event);

            person.update(cx, |person, cx| {
                person.set_announcement(announcement);
                cx.notify();
            });
        }
    }

    /// Insert batch of persons
    fn bulk_inserts(&mut self, persons: Vec<Person>, cx: &mut Context<Self>) {
        for person in persons.into_iter() {
            self.persons.insert(person.public_key(), cx.new(|_| person));
        }
        cx.notify();
    }

    /// Insert or update a person
    pub fn insert(&mut self, person: Person, cx: &mut App) {
        let public_key = person.public_key();

        match self.persons.get(&public_key) {
            Some(this) => {
                this.update(cx, |this, cx| {
                    *this = person;
                    cx.notify();
                });
            }
            None => {
                self.persons.insert(public_key, cx.new(|_| person));
            }
        }
    }

    /// Get single person by public key
    pub fn get(&self, public_key: &PublicKey, cx: &App) -> Person {
        if let Some(person) = self.persons.get(public_key) {
            return person.read(cx).clone();
        }

        let public_key = *public_key;
        let mut seen = self.seen.borrow_mut();

        if seen.insert(public_key) {
            let sender = self.sender.clone();

            // Spawn background task to request metadata
            cx.background_spawn(async move {
                if let Err(e) = sender.send_async(public_key).await {
                    log::warn!("Failed to send public key for metadata request: {}", e);
                }
            })
            .detach();
        }

        // Return a temporary profile with default metadata
        Person::new(public_key, Metadata::default())
    }
}
