use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use account::Account;
use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::{EventUtils, BOOTSTRAP_RELAYS, METADATA_BATCH_LIMIT};
use encryption::Encryption;
use flume::Sender;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Subscription, Task};
pub use message::*;
use nostr_sdk::prelude::*;
pub use room::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::lock::RwLock;
use state::{initialized_at, EventTracker, NostrRegistry, GIFTWRAP_SUBSCRIPTION};

mod message;
mod room;

pub fn init(cx: &mut App) {
    ChatRegistry::set_global(cx.new(ChatRegistry::new), cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

/// Chat Registry
#[derive(Debug)]
pub struct ChatRegistry {
    /// Collection of all chat rooms
    pub rooms: Vec<Entity<Room>>,

    /// Loading status of the registry
    pub loading: bool,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChatEvent {
    OpenRoom(u64),
    CloseRoom(u64),
    NewChatRequest(RoomKind),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signal {
    Loading(bool),
    Message(NewMessage),
    Eose,
}

impl EventEmitter<ChatEvent> for ChatRegistry {}

impl ChatRegistry {
    /// Retrieve the global chat registry state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalChatRegistry>().0.clone()
    }

    /// Set the global chat registry instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalChatRegistry(state));
    }

    /// Create a new chat registry instance
    fn new(cx: &mut Context<Self>) -> Self {
        let encryption = Encryption::global(cx);
        let encryption_key = encryption.read(cx).encryption.clone();

        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let tracker = nostr.read(cx).tracker();

        let status = Arc::new(AtomicBool::new(true));
        let (tx, rx) = flume::bounded::<Signal>(2048);

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the encryption global state
            cx.observe(&encryption_key, {
                let status = Arc::clone(&status);
                let tx = tx.clone();

                move |this, state, cx| {
                    if let Some(signer) = state.read(cx).clone() {
                        this.retry_failed_events(&signer, &tx, &status, cx);
                    }
                }
            }),
        );

        tasks.push(
            // Handle notifications
            cx.background_spawn({
                let client = nostr.read(cx).client();
                let status = Arc::clone(&status);
                let tx = tx.clone();

                async move { Self::handle_notifications(&client, &tracker, &tx, &status).await }
            }),
        );

        tasks.push(
            // Handle unwrapping status
            cx.background_spawn(
                async move { Self::handle_unwrapping(&client, &status, &tx).await },
            ),
        );

        tasks.push(
            // Handle new messages
            cx.spawn(async move |this, cx| {
                while let Ok(message) = rx.recv_async().await {
                    match message {
                        Signal::Message(message) => {
                            this.update(cx, |this, cx| {
                                this.new_message(message, cx);
                            })
                            .expect("Entity has been released");
                        }
                        Signal::Eose => {
                            this.update(cx, |this, cx| {
                                this.get_rooms(cx);
                            })
                            .expect("Entity has been released");
                        }
                        Signal::Loading(status) => {
                            this.update(cx, |this, cx| {
                                this.set_loading(status, cx);
                                this.get_rooms(cx);
                            })
                            .expect("Entity has been released");
                        }
                    };
                }
            }),
        );

        Self {
            rooms: vec![],
            loading: true,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn handle_notifications(
        client: &Client,
        tracker: &Arc<RwLock<EventTracker>>,
        tx: &Sender<Signal>,
        status: &Arc<AtomicBool>,
    ) {
        let initialized_at = initialized_at();
        let subscription_id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);

        let mut notifications = client.notifications();
        let mut public_keys = HashSet::new();
        let mut processed_events = HashSet::new();

        while let Ok(notification) = notifications.recv().await {
            let RelayPoolNotification::Message { message, .. } = notification else {
                // Skip non-message notifications
                continue;
            };

            match message {
                RelayMessage::Event { event, .. } => {
                    if !processed_events.insert(event.id) {
                        // Skip if the event has already been processed
                        continue;
                    }

                    if event.kind != Kind::GiftWrap {
                        // Skip non-gift wrap events
                        continue;
                    }

                    // Extract the rumor from the gift wrap event
                    match Self::extract_rumor(client, event.as_ref()).await {
                        Ok(rumor) => {
                            // Get all public keys
                            public_keys.extend(rumor.all_pubkeys());

                            let limit_reached = public_keys.len() >= METADATA_BATCH_LIMIT;
                            let done = !status.load(Ordering::Acquire) && !public_keys.is_empty();

                            // Get metadata for all public keys if the limit is reached
                            if limit_reached || done {
                                let public_keys = std::mem::take(&mut public_keys);
                                // Get metadata for the public keys
                                Self::get_metadata(client, public_keys).await.ok();
                            }

                            match &rumor.created_at >= initialized_at {
                                true => {
                                    let new_message = NewMessage::new(event.id, rumor);
                                    let signal = Signal::Message(new_message);

                                    if let Err(e) = tx.send_async(signal).await {
                                        log::error!("Failed to send signal: {}", e);
                                    }
                                }
                                false => {
                                    status.store(true, Ordering::Release);
                                }
                            }
                        }
                        Err(_e) => {
                            let mut tracker = tracker.write().await;
                            tracker.failed_unwrap_events.push(event.as_ref().clone());

                            drop(tracker);
                        }
                    }
                }
                RelayMessage::EndOfStoredEvents(id) => {
                    if id.as_ref() == &subscription_id {
                        if let Err(e) = tx.send_async(Signal::Eose).await {
                            log::error!("Failed to send signal: {}", e);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    async fn handle_unwrapping(client: &Client, status: &Arc<AtomicBool>, tx: &Sender<Signal>) {
        let loop_duration = Duration::from_secs(20);
        let mut is_start_processing = false;
        let mut total_loops = 0;

        loop {
            if client.has_signer().await {
                total_loops += 1;

                if status.load(Ordering::Acquire) {
                    is_start_processing = true;

                    // Reset gift wrap processing flag
                    _ = status.compare_exchange(true, false, Ordering::Release, Ordering::Relaxed);

                    // Send loading signal
                    if let Err(e) = tx.send_async(Signal::Loading(true)).await {
                        log::error!("Failed to send signal: {}", e);
                    }
                } else {
                    // Only run further if we are already processing
                    // Wait until after 2 loops to prevent exiting early while events are still being processed
                    if is_start_processing && total_loops >= 2 {
                        // Send loading signal
                        if let Err(e) = tx.send_async(Signal::Loading(false)).await {
                            log::error!("Failed to send signal: {}", e);
                        }
                        // Reset the counter
                        is_start_processing = false;
                        total_loops = 0;
                    }
                }
            }
            smol::Timer::after(loop_duration).await;
        }
    }

    fn retry_failed_events(
        &mut self,
        signer: &Arc<dyn NostrSigner>,
        tx: &Sender<Signal>,
        status: &Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let tracker = nostr.read(cx).tracker();

        let signer = Arc::clone(signer);
        let status = Arc::clone(status);

        let tx = tx.clone();
        let initialized_at = initialized_at();

        self._tasks.push(cx.background_spawn(async move {
            let tracker = tracker.read().await;

            for event in tracker.failed_unwrap_events.iter() {
                if let Ok(rumor) = Self::try_unwrap_custom(&client, &signer, event).await {
                    match &event.created_at >= initialized_at {
                        true => {
                            let new_message = NewMessage::new(event.id, rumor);
                            let signal = Signal::Message(new_message);

                            if let Err(e) = tx.send_async(signal).await {
                                log::error!("Failed to send signal: {}", e);
                            }
                        }
                        false => {
                            status.store(true, Ordering::Release);
                        }
                    }
                }
            }
        }));
    }

    /// Set the loading status of the chat registry
    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    /// Get a room by its ID.
    pub fn room(&self, id: &u64, cx: &App) -> Option<Entity<Room>> {
        self.rooms
            .iter()
            .find(|model| model.read(cx).id == *id)
            .cloned()
    }

    /// Get all ongoing rooms.
    pub fn ongoing_rooms(&self, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).kind == RoomKind::Ongoing)
            .cloned()
            .collect()
    }

    /// Get all request rooms.
    pub fn request_rooms(&self, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).kind != RoomKind::Ongoing)
            .cloned()
            .collect()
    }

    /// Add a new room to the start of list.
    pub fn add_room(&mut self, room: Entity<Room>, cx: &mut Context<Self>) {
        self.rooms.insert(0, room);
        cx.notify();
    }

    /// Close a room.
    pub fn close_room(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.rooms.iter().any(|r| r.read(cx).id == id) {
            cx.emit(ChatEvent::CloseRoom(id));
        }
    }

    /// Sort rooms by their created at.
    pub fn sort(&mut self, cx: &mut Context<Self>) {
        self.rooms.sort_by_key(|ev| Reverse(ev.read(cx).created_at));
        cx.notify();
    }

    /// Search rooms by their name.
    pub fn search(&self, query: &str, cx: &App) -> Vec<Entity<Room>> {
        let matcher = SkimMatcherV2::default();

        self.rooms
            .iter()
            .filter(|room| {
                matcher
                    .fuzzy_match(room.read(cx).display_name(cx).as_ref(), query)
                    .is_some()
            })
            .cloned()
            .collect()
    }

    /// Search rooms by public keys.
    pub fn search_by_public_key(&self, public_key: PublicKey, cx: &App) -> Vec<Entity<Room>> {
        self.rooms
            .iter()
            .filter(|room| room.read(cx).members.contains(&public_key))
            .cloned()
            .collect()
    }

    /// Reset the registry.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.rooms.clear();
        cx.notify();
    }

    /// Push a new room to the chat registry
    pub fn push_room(&mut self, room: Entity<Room>, cx: &mut Context<Self>) {
        let id = room.read(cx).id;

        if !self.rooms.iter().any(|r| r.read(cx).id == id) {
            self.add_room(room, cx);
        }

        cx.emit(ChatEvent::OpenRoom(id));
    }

    /// Extend the registry with new rooms.
    fn extend_rooms(&mut self, rooms: HashSet<Room>, cx: &mut Context<Self>) {
        let mut room_map: HashMap<u64, usize> = self
            .rooms
            .iter()
            .enumerate()
            .map(|(idx, room)| (room.read(cx).id, idx))
            .collect();

        for new_room in rooms.into_iter() {
            // Check if we already have a room with this ID
            if let Some(&index) = room_map.get(&new_room.id) {
                self.rooms[index].update(cx, |this, cx| {
                    if new_room.created_at > this.created_at {
                        *this = new_room;
                        cx.notify();
                    }
                });
            } else {
                let new_room_id = new_room.id;
                self.rooms.push(cx.new(|_| new_room));

                let new_index = self.rooms.len();
                room_map.insert(new_room_id, new_index);
            }
        }
    }

    /// Load all rooms from the database.
    pub fn get_rooms(&mut self, cx: &mut Context<Self>) {
        let task = self.create_get_rooms_task(cx);

        self._tasks.push(
            // Run and finished in the background
            cx.spawn(async move |this, cx| {
                match task.await {
                    Ok(rooms) => {
                        this.update(cx, move |this, cx| {
                            this.extend_rooms(rooms, cx);
                            this.sort(cx);
                        })
                        .expect("Entity has been released");
                    }
                    Err(e) => {
                        log::error!("Failed to load rooms: {e}")
                    }
                };
            }),
        );
    }

    /// Create a task to load rooms from the database
    fn create_get_rooms_task(&self, cx: &App) -> Task<Result<HashSet<Room>, Error>> {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Get the contact bypass setting
        let bypass_setting = AppSettings::get_contact_bypass(cx);

        cx.background_spawn(async move {
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let contacts = client.database().contacts_public_keys(public_key).await?;

            let authored_filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .custom_tag(SingleLetterTag::lowercase(Alphabet::A), public_key);

            // Get all authored events
            let authored = client.database().query(authored_filter).await?;

            let addressed_filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .custom_tag(SingleLetterTag::lowercase(Alphabet::P), public_key);

            // Get all addressed events
            let addressed = client.database().query(addressed_filter).await?;

            // Merge authored and addressed events
            let events = authored.merge(addressed);

            let mut rooms: HashSet<Room> = HashSet::new();
            let mut grouped: HashMap<u64, Vec<UnsignedEvent>> = HashMap::new();

            // Process each event and group by room hash
            for raw in events.into_iter() {
                if let Ok(rumor) = UnsignedEvent::from_json(&raw.content) {
                    if rumor.tags.public_keys().peekable().peek().is_some() {
                        grouped.entry(rumor.uniq_id()).or_default().push(rumor);
                    }
                }
            }

            for (_id, mut messages) in grouped.into_iter() {
                messages.sort_by_key(|m| Reverse(m.created_at));

                let Some(latest) = messages.first() else {
                    continue;
                };

                let mut room = Room::from(latest);

                if rooms.iter().any(|r| r.id == room.id) {
                    continue;
                }

                let mut public_keys = room.members();
                public_keys.retain(|pk| pk != &public_key);

                // Check if the user has responded to the room
                let user_sent = messages.iter().any(|m| m.pubkey == public_key);

                // Determine if the room is ongoing or not
                let mut bypassed = false;

                // Check if public keys are from the user's contacts
                if bypass_setting {
                    bypassed = public_keys.iter().any(|k| contacts.contains(k));
                }

                // Set the room's kind based on status
                if user_sent || bypassed {
                    room = room.kind(RoomKind::Ongoing);
                }

                rooms.insert(room);
            }

            Ok(rooms)
        })
    }

    /// Trigger a refresh of the opened chat rooms by their IDs
    pub fn refresh_rooms(&mut self, ids: Option<Vec<u64>>, cx: &mut Context<Self>) {
        if let Some(ids) = ids {
            for room in self.rooms.iter() {
                if ids.contains(&room.read(cx).id) {
                    room.update(cx, |this, cx| {
                        this.emit_refresh(cx);
                    });
                }
            }
        }
    }

    /// Parse a Nostr event into a Coop Message and push it to the belonging room
    ///
    /// If the room doesn't exist, it will be created.
    /// Updates room ordering based on the most recent messages.
    pub fn new_message(&mut self, message: NewMessage, cx: &mut Context<Self>) {
        let id = message.rumor.uniq_id();
        let author = message.rumor.pubkey;
        let account = Account::global(cx);

        if let Some(room) = self.rooms.iter().find(|room| room.read(cx).id == id) {
            let is_new_event = message.rumor.created_at > room.read(cx).created_at;
            let created_at = message.rumor.created_at;
            let event_for_emit = message.rumor.clone();

            // Update room
            room.update(cx, |this, cx| {
                if is_new_event {
                    this.set_created_at(created_at, cx);
                }

                // Set this room is ongoing if the new message is from current user
                if author == account.read(cx).public_key() {
                    this.set_ongoing(cx);
                }

                // Emit the new message to the room
                this.emit_message(message.gift_wrap, event_for_emit.clone(), cx);
            });

            // Resort all rooms in the registry by their created at (after updated)
            if is_new_event {
                self.sort(cx);
            }
        } else {
            // Push the new room to the front of the list
            self.add_room(cx.new(|_| Room::from(&message.rumor)), cx);

            // Notify the UI about the new room
            cx.emit(ChatEvent::NewChatRequest(RoomKind::default()));
        }
    }

    // Unwraps a gift-wrapped event and processes its contents.
    async fn extract_rumor(client: &Client, gift_wrap: &Event) -> Result<UnsignedEvent, Error> {
        // Try to get cached rumor first
        if let Ok(event) = Self::get_rumor(client, gift_wrap.id).await {
            return Ok(event);
        }

        // Try to unwrap with the available signer
        let unwrapped = Self::try_unwrap(client, gift_wrap).await?;
        let mut rumor_unsigned = unwrapped.rumor;

        // Generate event id for the rumor if it doesn't have one
        rumor_unsigned.ensure_id();

        // Cache the rumor
        Self::set_rumor(client, gift_wrap.id, &rumor_unsigned).await?;

        Ok(rumor_unsigned)
    }

    // Helper method to try unwrapping with different signers
    async fn try_unwrap(client: &Client, gift_wrap: &Event) -> Result<UnwrappedGift, Error> {
        let signer = client.signer().await?;
        let unwrapped = UnwrappedGift::from_gift_wrap(&signer, gift_wrap).await?;

        Ok(unwrapped)
    }

    /// Helper method to try unwrapping with a custom signer
    async fn try_unwrap_custom<T>(
        client: &Client,
        signer: &T,
        gift_wrap: &Event,
    ) -> Result<UnsignedEvent, Error>
    where
        T: NostrSigner,
    {
        let unwrapped = UnwrappedGift::from_gift_wrap(signer, gift_wrap).await?;
        let mut rumor_unsigned = unwrapped.rumor;

        // Generate event id for the rumor if it doesn't have one
        rumor_unsigned.ensure_id();

        // Cache the rumor
        Self::set_rumor(client, gift_wrap.id, &rumor_unsigned).await?;

        Ok(rumor_unsigned)
    }

    /// Stores an unwrapped event in local database with reference to original
    async fn set_rumor(client: &Client, id: EventId, rumor: &UnsignedEvent) -> Result<(), Error> {
        let rumor_id = rumor.id.context("Rumor is missing an event id")?;
        let author = rumor.pubkey;
        let conversation = Self::conversation_id(rumor);

        let mut tags = rumor.tags.clone().to_vec();

        // Add a unique identifier
        tags.push(Tag::identifier(id));

        // Add a reference to the rumor's author
        tags.push(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)),
            [author],
        ));

        // Add a conversation id
        tags.push(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::C)),
            [conversation.to_string()],
        ));

        // Add a reference to the rumor's id
        tags.push(Tag::event(rumor_id));

        // Add references to the rumor's participants
        for receiver in rumor.tags.public_keys().copied() {
            tags.push(Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                [receiver],
            ));
        }

        // Convert rumor to json
        let content = rumor.as_json();

        // Construct the event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, content)
            .tags(tags)
            .sign(&Keys::generate())
            .await?;

        // Save the event to the database
        client.database().save_event(&event).await?;

        Ok(())
    }

    /// Retrieves a previously unwrapped event from local database
    async fn get_rumor(client: &Client, gift_wrap: EventId) -> Result<UnsignedEvent, Error> {
        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(gift_wrap)
            .limit(1);

        if let Some(event) = client.database().query(filter).await?.first_owned() {
            UnsignedEvent::from_json(event.content).map_err(|e| anyhow!(e))
        } else {
            Err(anyhow!("Event is not cached yet."))
        }
    }

    /// Get metadata for a list of public keys
    async fn get_metadata<I>(client: &Client, public_keys: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = PublicKey>,
    {
        let authors: Vec<PublicKey> = public_keys.into_iter().collect();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList];

        // Return if the list is empty
        if authors.is_empty() {
            return Err(anyhow!("You need at least one public key".to_string(),));
        }

        let filter = Filter::new()
            .limit(authors.len() * kinds.len())
            .authors(authors)
            .kinds(kinds);

        // Subscribe to filters to the bootstrap relays
        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    /// Get the conversation ID for a given rumor (message).
    fn conversation_id(rumor: &UnsignedEvent) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut pubkeys: Vec<PublicKey> = rumor.tags.public_keys().copied().collect();
        pubkeys.push(rumor.pubkey);
        pubkeys.sort();
        pubkeys.dedup();
        pubkeys.hash(&mut hasher);

        hasher.finish()
    }
}
