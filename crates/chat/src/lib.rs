use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as AnyhowContext, Error};
use common::EventUtils;
use device::DeviceRegistry;
use flume::Sender;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, Global, Subscription, Task, WeakEntity,
};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use state::{tracker, NostrRegistry, GIFTWRAP_SUBSCRIPTION};

mod message;
mod room;

pub use message::*;
pub use room::*;

pub fn init(cx: &mut App) {
    ChatRegistry::set_global(cx.new(ChatRegistry::new), cx);
}

struct GlobalChatRegistry(Entity<ChatRegistry>);

impl Global for GlobalChatRegistry {}

/// Chat event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChatEvent {
    /// An event to open a room by its ID
    OpenRoom(u64),
    /// An event to close a room by its ID
    CloseRoom(u64),
    /// An event to notify UI about a new chat request
    Ping,
}

/// Channel signal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NostrEvent {
    /// Message received from relay pool
    Message(NewMessage),
    /// Unwrapping status
    Unwrapping(bool),
    /// Eose received from relay pool
    Eose,
}

/// Chat Registry
#[derive(Debug)]
pub struct ChatRegistry {
    /// Collection of all chat rooms
    rooms: Vec<Entity<Room>>,

    /// Loading status of the registry
    loading: bool,

    /// Tracking the status of unwrapping gift wrap events.
    tracking_flag: Arc<AtomicBool>,

    /// Channel's sender for communication between nostr and gpui
    sender: Sender<NostrEvent>,

    /// Handle notifications asynchronous task
    notifications: Option<Task<Result<(), Error>>>,

    /// Tasks for asynchronous operations
    tasks: Vec<Task<()>>,

    /// Subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,
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
        let nostr = NostrRegistry::global(cx);
        let identity = nostr.read(cx).identity();

        let device = DeviceRegistry::global(cx);
        let device_signer = device.read(cx).device_signer.clone();

        // A flag to indicate if the registry is loading
        let tracking_flag = Arc::new(AtomicBool::new(true));

        // Channel for communication between nostr and gpui
        let (tx, rx) = flume::bounded::<NostrEvent>(2048);

        let mut tasks = vec![];
        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Observe the identity
            cx.observe(&identity, |this, state, cx| {
                if state.read(cx).has_public_key() {
                    // Handle nostr notifications
                    this.handle_notifications(cx);
                    // Track unwrapping progress
                    this.tracking(cx);
                }
            }),
        );

        subscriptions.push(
            // Observe the device signer state
            cx.observe(&device_signer, |this, state, cx| {
                if state.read(cx).is_some() {
                    this.handle_notifications(cx);
                }
            }),
        );

        tasks.push(
            // Update GPUI states
            cx.spawn(async move |this, cx| {
                while let Ok(message) = rx.recv_async().await {
                    match message {
                        NostrEvent::Message(message) => {
                            this.update(cx, |this, cx| {
                                this.new_message(message, cx);
                            })
                            .ok();
                        }
                        NostrEvent::Eose => {
                            this.update(cx, |this, cx| {
                                this.get_rooms(cx);
                            })
                            .ok();
                        }
                        NostrEvent::Unwrapping(status) => {
                            this.update(cx, |this, cx| {
                                this.set_loading(status, cx);
                                this.get_rooms(cx);
                            })
                            .ok();
                        }
                    };
                }
            }),
        );

        Self {
            rooms: vec![],
            loading: true,
            tracking_flag,
            sender: tx.clone(),
            notifications: None,
            tasks,
            _subscriptions: subscriptions,
        }
    }

    /// Handle nostr notifications
    fn handle_notifications(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let device = DeviceRegistry::global(cx);
        let device_signer = device.read(cx).signer(cx);

        let status = self.tracking_flag.clone();
        let tx = self.sender.clone();

        self.tasks.push(cx.background_spawn(async move {
            let initialized_at = Timestamp::now();
            let subscription_id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);

            let mut notifications = client.notifications();
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
                        match Self::extract_rumor(&client, &device_signer, event.as_ref()).await {
                            Ok(rumor) => match rumor.created_at >= initialized_at {
                                true => {
                                    // Check if the event is sent by coop
                                    let sent_by_coop = {
                                        let tracker = tracker().read().await;
                                        tracker.is_sent_by_coop(&event.id)
                                    };
                                    // No need to emit if sent by coop
                                    // the event is already emitted
                                    if !sent_by_coop {
                                        let new_message = NewMessage::new(event.id, rumor);
                                        let signal = NostrEvent::Message(new_message);

                                        tx.send_async(signal).await.ok();
                                    }
                                }
                                false => {
                                    status.store(true, Ordering::Release);
                                }
                            },
                            Err(e) => {
                                log::warn!("Failed to unwrap: {e}");
                            }
                        }
                    }
                    RelayMessage::EndOfStoredEvents(id) => {
                        if id.as_ref() == &subscription_id {
                            tx.send_async(NostrEvent::Eose).await.ok();
                        }
                    }
                    _ => {}
                }
            }
        }));
    }

    /// Tracking the status of unwrapping gift wrap events.
    fn tracking(&mut self, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let status = self.tracking_flag.clone();
        let tx = self.sender.clone();

        self.notifications = Some(cx.background_spawn(async move {
            let loop_duration = Duration::from_secs(12);

            let mut is_start_processing = false;
            let mut total_loops = 0;

            loop {
                if client.has_signer().await {
                    total_loops += 1;

                    if status.load(Ordering::Acquire) {
                        is_start_processing = true;
                        // Reset gift wrap processing flag
                        _ = status.compare_exchange(
                            true,
                            false,
                            Ordering::Release,
                            Ordering::Relaxed,
                        );

                        tx.send_async(NostrEvent::Unwrapping(true)).await.ok();
                    } else {
                        // Only run further if we are already processing
                        // Wait until after 2 loops to prevent exiting early while events are still being processed
                        if is_start_processing && total_loops >= 2 {
                            tx.send_async(NostrEvent::Unwrapping(false)).await.ok();

                            // Reset the counter
                            is_start_processing = false;
                            total_loops = 0;
                        }
                    }
                }
                smol::Timer::after(loop_duration).await;
            }
        }));
    }

    /// Get the loading status of the chat registry
    pub fn loading(&self) -> bool {
        self.loading
    }

    /// Set the loading status of the chat registry
    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    /// Get a weak reference to a room by its ID.
    pub fn room(&self, id: &u64, cx: &App) -> Option<WeakEntity<Room>> {
        self.rooms
            .iter()
            .find(|this| &this.read(cx).id == id)
            .map(|this| this.downgrade())
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
    pub fn add_room<I>(&mut self, room: I, cx: &mut Context<Self>)
    where
        I: Into<Room>,
    {
        self.rooms.insert(0, cx.new(|_| room.into()));
        cx.notify();
    }

    /// Emit an open room event.
    /// If the room is new, add it to the registry.
    pub fn emit_room(&mut self, room: WeakEntity<Room>, cx: &mut Context<Self>) {
        if let Some(room) = room.upgrade() {
            let id = room.read(cx).id;

            // If the room is new, add it to the registry.
            if !self.rooms.iter().any(|r| r.read(cx).id == id) {
                self.rooms.insert(0, room);
            }

            // Emit the open room event.
            cx.emit(ChatEvent::OpenRoom(id));
        }
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

        self.tasks.push(
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
        let nostr = NostrRegistry::global(cx);
        // Get the unique id
        let id = message.rumor.uniq_id();
        // Get the author
        let author = message.rumor.pubkey;

        match self.rooms.iter().find(|room| room.read(cx).id == id) {
            Some(room) => {
                let new_message = message.rumor.created_at > room.read(cx).created_at;
                let created_at = message.rumor.created_at;

                // Update room
                room.update(cx, |this, cx| {
                    // Update the last timestamp if the new message is newer
                    if new_message {
                        this.set_created_at(created_at, cx);
                    }

                    // Set this room is ongoing if the new message is from current user
                    if author == nostr.read(cx).identity().read(cx).public_key() {
                        this.set_ongoing(cx);
                    }

                    // Emit the new message to the room
                    this.emit_message(message, cx);
                });

                // Resort all rooms in the registry by their created at (after updated)
                if new_message {
                    self.sort(cx);
                }
            }
            None => {
                // Push the new room to the front of the list
                self.add_room(&message.rumor, cx);

                // Notify the UI about the new room
                cx.emit(ChatEvent::Ping);
            }
        }
    }

    // Unwraps a gift-wrapped event and processes its contents.
    async fn extract_rumor(
        client: &Client,
        device_signer: &Option<Arc<dyn NostrSigner>>,
        gift_wrap: &Event,
    ) -> Result<UnsignedEvent, Error> {
        // Try to get cached rumor first
        if let Ok(event) = Self::get_rumor(client, gift_wrap.id).await {
            return Ok(event);
        }

        // Try to unwrap with the available signer
        let unwrapped = Self::try_unwrap(client, device_signer, gift_wrap).await?;
        let mut rumor_unsigned = unwrapped.rumor;

        // Generate event id for the rumor if it doesn't have one
        rumor_unsigned.ensure_id();

        // Cache the rumor
        Self::set_rumor(client, gift_wrap.id, &rumor_unsigned).await?;

        Ok(rumor_unsigned)
    }

    // Helper method to try unwrapping with different signers
    async fn try_unwrap(
        client: &Client,
        device_signer: &Option<Arc<dyn NostrSigner>>,
        gift_wrap: &Event,
    ) -> Result<UnwrappedGift, Error> {
        if let Some(signer) = device_signer.as_ref() {
            let seal = signer
                .nip44_decrypt(&gift_wrap.pubkey, &gift_wrap.content)
                .await?;

            let seal: Event = Event::from_json(seal)?;
            seal.verify_with_ctx(&SECP256K1)?;

            let rumor = signer.nip44_decrypt(&seal.pubkey, &seal.content).await?;
            let rumor = UnsignedEvent::from_json(rumor)?;

            return Ok(UnwrappedGift {
                sender: seal.pubkey,
                rumor,
            });
        }

        let signer = client.signer().await?;
        let unwrapped = UnwrappedGift::from_gift_wrap(&signer, gift_wrap).await?;

        Ok(unwrapped)
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
