use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Error};
use auto_update::AutoUpdater;
use client_keys::ClientKeys;
use common::display::ReadableProfile;
use common::event::EventUtils;
use flume::{Receiver, Sender};
use global::constants::{
    ACCOUNT_IDENTIFIER, BOOTSTRAP_RELAYS, DEFAULT_SIDEBAR_WIDTH, METADATA_BATCH_LIMIT,
    METADATA_BATCH_TIMEOUT, SEARCH_RELAYS,
};
use global::{css, ingester, nostr_client, AuthRequest, Notice, Signal, UnwrappingStatus};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, rems, App, AppContext, AsyncWindowContext, Axis, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, WeakEntity, Window,
};
use i18n::{shared_t, t};
use itertools::Itertools;
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use registry::{Registry, RegistryEvent};
use settings::AppSettings;
use signer_proxy::{BrowserSignerProxy, BrowserSignerProxyOptions};
use smallvec::{smallvec, SmallVec};
use theme::{ActiveTheme, Theme, ThemeMode};
use title_bar::TitleBar;
use ui::actions::OpenProfile;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{ClosePanel, DockArea, DockItem};
use ui::modal::ModalButtonProps;
use ui::notification::Notification;
use ui::popup_menu::PopupMenuExt;
use ui::{h_flex, v_flex, ContextModal, Disableable, IconName, Root, Sizable, StyledExt};

use crate::actions::{DarkMode, Logout, ReloadMetadata, Settings};
use crate::views::compose::compose_button;
use crate::views::setup_relay::setup_nip17_relay;
use crate::views::{
    account, chat, login, new_account, onboarding, preferences, sidebar, user_profile, welcome,
};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<ChatSpace> {
    cx.new(|cx| ChatSpace::new(window, cx))
}

pub fn login(window: &mut Window, cx: &mut App) {
    let panel = login::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

pub fn new_account(window: &mut Window, cx: &mut App) {
    let panel = new_account::init(window, cx);
    ChatSpace::set_center_panel(panel, window, cx);
}

pub struct ChatSpace {
    // Workspace
    title_bar: Entity<TitleBar>,
    dock: Entity<DockArea>,

    // Temporarily store all authentication requests
    auth_requests: HashMap<AuthRequest, bool>,

    // Local state to determine if the user has set up NIP-17 relays
    has_nip17_relays: bool,

    // System
    _subscriptions: SmallVec<[Subscription; 3]>,
    _tasks: SmallVec<[Task<()>; 5]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client_keys = ClientKeys::global(cx);
        let registry = Registry::global(cx);
        let status = registry.read(cx).unwrapping_status.clone();

        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| DockArea::new(window, cx));

        let (pubkey_tx, pubkey_rx) = flume::bounded::<PublicKey>(1024);
        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the client keys and show an alert modal if they fail to initialize
            cx.observe_in(&client_keys, window, |this, keys, window, cx| {
                if !keys.read(cx).has_keys() {
                    this.render_client_keys_modal(window, cx);
                } else {
                    this.load_local_account(window, cx);
                }
            }),
        );

        subscriptions.push(
            // Observe the global registry
            cx.observe_in(&status, window, move |this, status, window, cx| {
                let registry = Registry::global(cx);
                let status = status.read(cx);
                let all_panels = this.get_all_panel_ids(cx);

                match status {
                    UnwrappingStatus::Processing => {
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                            this.refresh_rooms(all_panels, cx);
                        });
                    }
                    UnwrappingStatus::Complete => {
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                            this.refresh_rooms(all_panels, cx);
                        });
                    }
                    _ => {}
                };
            }),
        );

        subscriptions.push(
            // Subscribe to open chat room requests
            cx.subscribe_in(&registry, window, move |this, _, event, window, cx| {
                this.process_registry_event(event, window, cx);
            }),
        );

        tasks.push(
            // Connect to the bootstrap relays
            // Then handle nostr events in the background
            cx.background_spawn(async move {
                Self::connect()
                    .await
                    .expect("Failed connect the bootstrap relays. Please restart the application.");

                Self::process_nostr_events(&pubkey_tx)
                    .await
                    .expect("Failed to handle nostr events. Please restart the application.");
            }),
        );

        tasks.push(
            // Wait for the signer to be set
            // Also verify NIP-65 and NIP-17 relays after the signer is set
            cx.background_spawn(async move {
                Self::observe_signer().await;
            }),
        );

        tasks.push(
            // Observe gift wrap process in the background
            cx.background_spawn(async move {
                Self::observe_giftwrap().await;
            }),
        );

        tasks.push(
            // Listen all metadata requests then batch them into single subscription
            cx.background_spawn(async move {
                Self::process_batching_metadata(&pubkey_rx).await;
            }),
        );

        tasks.push(
            // Continuously handle signals from the Nostr channel
            cx.spawn_in(window, async move |this, cx| {
                Self::process_nostr_signals(this, cx).await
            }),
        );

        Self {
            dock,
            title_bar,
            auth_requests: HashMap::new(),
            has_nip17_relays: true,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn connect() -> Result<(), Error> {
        let client = nostr_client();

        for relay in BOOTSTRAP_RELAYS.into_iter() {
            client.add_relay(relay).await?;
        }

        log::info!("Connected to bootstrap relays");

        for relay in SEARCH_RELAYS.into_iter() {
            client.add_relay(relay).await?;
        }

        log::info!("Connected to search relays");

        // Establish connection to relays
        client.connect().await;

        Ok(())
    }

    async fn observe_signer() {
        let client = nostr_client();
        let ingester = ingester();
        let loop_duration = Duration::from_secs(1);
        let mut is_sent_signal = false;
        let mut identity: Option<PublicKey> = None;

        loop {
            if let Some(public_key) = identity {
                let nip65 = Filter::new().kind(Kind::RelayList).author(public_key);

                if client.database().count(nip65).await.unwrap_or(0) > 0 {
                    let dm_relays = Filter::new().kind(Kind::InboxRelays).author(public_key);

                    match client.database().query(dm_relays).await {
                        Ok(events) => {
                            if let Some(event) = events.first_owned() {
                                let relay_urls = nip17::extract_relay_list(&event).collect_vec();

                                if relay_urls.is_empty() {
                                    if !is_sent_signal {
                                        ingester.send(Signal::DmRelayNotFound).await;
                                        is_sent_signal = true;
                                    }
                                } else {
                                    break;
                                }
                            } else if !is_sent_signal {
                                ingester.send(Signal::DmRelayNotFound).await;
                                is_sent_signal = true;
                            } else {
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("Database query error: {e}");
                            if !is_sent_signal {
                                ingester.send(Signal::DmRelayNotFound).await;
                                is_sent_signal = true;
                            }
                        }
                    }
                } else {
                    log::error!("Database error.");
                    break;
                }
            } else {
                // Wait for signer set
                if let Ok(signer) = client.signer().await {
                    if let Ok(public_key) = signer.get_public_key().await {
                        identity = Some(public_key);

                        // Notify the app that the signer has been set.
                        ingester.send(Signal::SignerSet(public_key)).await;

                        // Subscribe to the NIP-65 relays for the public key.
                        if let Err(e) = Self::fetch_nip65_relays(public_key).await {
                            log::error!("Failed to fetch NIP-65 relays: {e}");
                        }
                    }
                }
            }

            smol::Timer::after(loop_duration).await;
        }
    }

    async fn observe_giftwrap() {
        let client = nostr_client();
        let css = css();
        let ingester = ingester();
        let loop_duration = Duration::from_secs(20);
        let mut is_start_processing = false;
        let mut total_loops = 0;

        loop {
            if client.has_signer().await {
                total_loops += 1;

                if css.gift_wrap_processing.load(Ordering::Acquire) {
                    is_start_processing = true;

                    // Reset gift wrap processing flag
                    let _ = css.gift_wrap_processing.compare_exchange(
                        true,
                        false,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );

                    let signal = Signal::GiftWrapProcess(UnwrappingStatus::Processing);
                    ingester.send(signal).await;
                } else {
                    // Only run further if we are already processing
                    // Wait until after 2 loops to prevent exiting early while events are still being processed
                    if is_start_processing && total_loops >= 2 {
                        let signal = Signal::GiftWrapProcess(UnwrappingStatus::Complete);
                        ingester.send(signal).await;

                        // Reset the counter
                        is_start_processing = false;
                        total_loops = 0;
                    }
                }
            }

            smol::Timer::after(loop_duration).await;
        }
    }

    async fn process_batching_metadata(rx: &Receiver<PublicKey>) {
        let timeout = Duration::from_millis(METADATA_BATCH_TIMEOUT);
        let mut processed_pubkeys: HashSet<PublicKey> = HashSet::new();
        let mut batch: HashSet<PublicKey> = HashSet::new();

        /// Internal events for the metadata batching system
        enum BatchEvent {
            PublicKey(PublicKey),
            Timeout,
            Closed,
        }

        loop {
            let futs = smol::future::or(
                async move {
                    if let Ok(public_key) = rx.recv_async().await {
                        BatchEvent::PublicKey(public_key)
                    } else {
                        BatchEvent::Closed
                    }
                },
                async move {
                    smol::Timer::after(timeout).await;
                    BatchEvent::Timeout
                },
            );

            match futs.await {
                BatchEvent::PublicKey(public_key) => {
                    // Prevent duplicate keys from being processed
                    if processed_pubkeys.insert(public_key) {
                        batch.insert(public_key);
                    }

                    // Process the batch if it's full
                    if batch.len() >= METADATA_BATCH_LIMIT {
                        Self::fetch_metadata_for_pubkeys(std::mem::take(&mut batch)).await;
                    }
                }
                BatchEvent::Timeout => {
                    Self::fetch_metadata_for_pubkeys(std::mem::take(&mut batch)).await;
                }
                BatchEvent::Closed => {
                    Self::fetch_metadata_for_pubkeys(std::mem::take(&mut batch)).await;
                    break;
                }
            }
        }
    }

    async fn process_nostr_events(pubkey_tx: &Sender<PublicKey>) -> Result<(), Error> {
        let client = nostr_client();
        let ingester = ingester();
        let css = css();

        let mut processed_events: HashSet<EventId> = HashSet::new();
        let mut challenges: HashSet<Cow<'_, str>> = HashSet::new();
        let mut notifications = client.notifications();

        while let Ok(notification) = notifications.recv().await {
            let RelayPoolNotification::Message { message, relay_url } = notification else {
                continue;
            };

            match message {
                RelayMessage::Event { event, .. } => {
                    // Skip events that have already been processed
                    if !processed_events.insert(event.id) {
                        continue;
                    }

                    match event.kind {
                        Kind::RelayList => {
                            if let Ok(true) = Self::is_self_event(&event).await {
                                // Fetch user's metadata event
                                Self::fetch_single_event(Kind::Metadata, event.pubkey).await;

                                // Fetch user's contact list event
                                Self::fetch_single_event(Kind::ContactList, event.pubkey).await;

                                // Fetch user's inbox relays event
                                Self::fetch_single_event(Kind::InboxRelays, event.pubkey).await;
                            }
                        }
                        Kind::InboxRelays => {
                            if let Ok(true) = Self::is_self_event(&event).await {
                                let relays = nip17::extract_relay_list(&event).collect_vec();

                                if !relays.is_empty() {
                                    for relay in relays.clone().into_iter() {
                                        if client.add_relay(relay).await.is_err() {
                                            let notice = Notice::RelayFailed(relay.clone());
                                            ingester.send(Signal::Notice(notice)).await;
                                        }
                                        if client.connect_relay(relay).await.is_err() {
                                            let notice = Notice::RelayFailed(relay.clone());
                                            ingester.send(Signal::Notice(notice)).await;
                                        }
                                    }

                                    // Subscribe to gift wrap events only in the current user's NIP-17 relays
                                    Self::fetch_gift_wrap(relays, event.pubkey).await;
                                }
                            }
                        }
                        Kind::ContactList => {
                            if let Ok(true) = Self::is_self_event(&event).await {
                                let public_keys = event.tags.public_keys().copied().collect_vec();
                                let kinds = vec![Kind::Metadata, Kind::ContactList];
                                let limit = public_keys.len() * kinds.len();
                                let filter =
                                    Filter::new().limit(limit).authors(public_keys).kinds(kinds);

                                client
                                    .subscribe_to(BOOTSTRAP_RELAYS, filter, css.auto_close_opts)
                                    .await
                                    .ok();
                            }
                        }
                        Kind::Metadata => {
                            if let Ok(metadata) = Metadata::from_json(&event.content) {
                                let profile = Profile::new(event.pubkey, metadata);
                                ingester.send(Signal::Metadata(profile)).await;
                            }
                        }
                        Kind::GiftWrap => {
                            Self::unwrap_gift_wrap(&event, pubkey_tx).await;
                        }
                        _ => {}
                    }
                }
                RelayMessage::EndOfStoredEvents(subscription_id) => {
                    if *subscription_id == css.gift_wrap_sub_id {
                        let signal = Signal::GiftWrapProcess(UnwrappingStatus::Processing);
                        ingester.send(signal).await;
                    }
                }
                RelayMessage::Auth { challenge } => {
                    if challenges.insert(challenge.clone()) {
                        let req = AuthRequest::new(challenge, relay_url);
                        // Send a signal to the ingester to handle the auth request
                        ingester.send(Signal::Auth(req)).await;
                    }
                }
                RelayMessage::Ok {
                    event_id, message, ..
                } => {
                    // Keep track of events sent by Coop
                    css.sent_ids.write().await.insert(event_id);

                    // Keep track of events that need to be resent
                    match MachineReadablePrefix::parse(&message) {
                        Some(MachineReadablePrefix::AuthRequired) => {
                            css.resend_queue.write().await.insert(event_id, relay_url);
                        }
                        Some(_) => {}
                        None => {}
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn process_nostr_signals(view: WeakEntity<ChatSpace>, cx: &mut AsyncWindowContext) {
        let ingester = ingester();
        let signals = ingester.signals();
        let mut is_open_proxy_modal = false;

        while let Ok(signal) = signals.recv_async().await {
            cx.update(|window, cx| {
                let registry = Registry::global(cx);
                let settings = AppSettings::global(cx);

                match signal {
                    Signal::SignerSet(public_key) => {
                        window.close_modal(cx);

                        // Setup the default layout for current workspace
                        view.update(cx, |this, cx| {
                            this.set_default_layout(window, cx);
                        })
                        .ok();

                        // Load user's settings
                        settings.update(cx, |this, cx| {
                            this.load_settings(cx);
                        });

                        // Load all chat rooms
                        registry.update(cx, |this, cx| {
                            this.set_identity(public_key, cx);
                            this.load_rooms(window, cx);
                        });
                    }
                    Signal::SignerUnset => {
                        // Setup the onboarding layout for current workspace
                        view.update(cx, |this, cx| {
                            this.set_onboarding_layout(window, cx);
                        })
                        .ok();

                        // Clear all current chat rooms
                        registry.update(cx, |this, cx| {
                            this.reset(cx);
                        });
                    }
                    Signal::Auth(req) => {
                        let url = &req.url;
                        let auto_auth = AppSettings::get_auto_auth(cx);
                        let is_authenticated = AppSettings::read_global(cx).is_authenticated(url);

                        view.update(cx, |this, cx| {
                            this.push_auth_request(&req, cx);

                            if auto_auth && is_authenticated {
                                // Automatically authenticate if the relay is authenticated before
                                this.auth(req, window, cx);
                            } else {
                                // Otherwise open the auth request popup
                                this.open_auth_request(req, window, cx);
                            }
                        })
                        .ok();
                    }
                    Signal::ProxyDown => {
                        if !is_open_proxy_modal {
                            is_open_proxy_modal = true;

                            view.update(cx, |this, cx| {
                                this.render_proxy_modal(window, cx);
                            })
                            .ok();
                        }
                    }
                    Signal::GiftWrapProcess(status) => {
                        registry.update(cx, |this, cx| {
                            this.set_unwrapping_status(status, cx);
                        });
                    }
                    Signal::Metadata(profile) => {
                        registry.update(cx, |this, cx| {
                            this.insert_or_update_person(profile, cx);
                        });
                    }
                    Signal::Message((gift_wrap_id, event)) => {
                        registry.update(cx, |this, cx| {
                            this.event_to_message(gift_wrap_id, event, window, cx);
                        });
                    }
                    Signal::DmRelayNotFound => {
                        view.update(cx, |this, cx| {
                            this.set_no_nip17_relays(cx);
                        })
                        .ok();
                    }
                    Signal::Notice(msg) => {
                        window.push_notification(msg.as_str(), cx);
                    }
                };
            })
            .ok();
        }
    }

    /// Checks if an event is belong to the current user
    async fn is_self_event(event: &Event) -> Result<bool, Error> {
        let client = nostr_client();
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        Ok(public_key == event.pubkey)
    }

    /// Fetches a single event by kind and public key
    pub async fn fetch_single_event(kind: Kind, public_key: PublicKey) {
        let client = nostr_client();
        let css = css();
        let filter = Filter::new().kind(kind).author(public_key).limit(1);

        if let Err(e) = client.subscribe(filter, css.auto_close_opts).await {
            log::info!("Failed to subscribe: {e}");
        }
    }

    pub async fn fetch_gift_wrap(relays: Vec<&RelayUrl>, public_key: PublicKey) {
        let client = nostr_client();
        let sub_id = css().gift_wrap_sub_id.clone();
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        if client
            .subscribe_with_id_to(relays.clone(), sub_id, filter, None)
            .await
            .is_ok()
        {
            log::info!("Subscribed to messages in: {relays:?}");
        }
    }

    /// Fetches NIP-65 relay list for a given public key
    pub async fn fetch_nip65_relays(public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let css = css();

        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, css.auto_close_opts)
            .await?;

        Ok(())
    }

    /// Fetches metadata for a list of public keys
    async fn fetch_metadata_for_pubkeys(public_keys: HashSet<PublicKey>) {
        if public_keys.is_empty() {
            return;
        }

        let client = nostr_client();
        let css = css();

        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];
        let limit = public_keys.len() * kinds.len() + 20;

        // A filter to fetch metadata
        let filter = Filter::new().authors(public_keys).kinds(kinds).limit(limit);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, css.auto_close_opts)
            .await
            .ok();
    }

    /// Stores an unwrapped event in local database with reference to original
    async fn set_unwrapped_event(root: EventId, unwrapped: &Event) -> Result<(), Error> {
        let client = nostr_client();

        // Save unwrapped event
        client.database().save_event(unwrapped).await?;

        // Create a reference event pointing to the unwrapped event
        let event = EventBuilder::new(Kind::ApplicationSpecificData, "")
            .tags(vec![Tag::identifier(root), Tag::event(unwrapped.id)])
            .sign(&Keys::generate())
            .await?;

        // Save reference event
        client.database().save_event(&event).await?;

        Ok(())
    }

    /// Retrieves a previously unwrapped event from local database
    async fn get_unwrapped_event(root: EventId) -> Result<Event, Error> {
        let client = nostr_client();
        let filter = Filter::new()
            .kind(Kind::ApplicationSpecificData)
            .identifier(root)
            .limit(1);

        if let Some(event) = client.database().query(filter).await?.first_owned() {
            let target_id = event.tags.event_ids().collect_vec()[0];

            if let Some(event) = client.database().event_by_id(target_id).await? {
                Ok(event)
            } else {
                Err(anyhow!("Event not found."))
            }
        } else {
            Err(anyhow!("Event is not cached yet."))
        }
    }

    /// Unwraps a gift-wrapped event and processes its contents.
    async fn unwrap_gift_wrap(target: &Event, pubkey_tx: &Sender<PublicKey>) {
        let client = nostr_client();
        let ingester = ingester();
        let css = css();
        let mut message: Option<Event> = None;

        if let Ok(event) = Self::get_unwrapped_event(target.id).await {
            message = Some(event);
        } else if let Ok(unwrapped) = client.unwrap_gift_wrap(target).await {
            // Sign the unwrapped event with a RANDOM KEYS
            if let Ok(event) = unwrapped.rumor.sign_with_keys(&Keys::generate()) {
                // Save this event to the database for future use.
                if let Err(e) = Self::set_unwrapped_event(target.id, &event).await {
                    log::warn!("Failed to cache unwrapped event: {e}")
                }

                message = Some(event);
            }
        }

        if let Some(event) = message {
            // Send all pubkeys to the metadata batch to sync data
            for public_key in event.all_pubkeys() {
                pubkey_tx.send_async(public_key).await.ok();
            }

            match event.created_at >= css.init_at {
                // New message: send a signal to notify the UI
                true => {
                    // Prevent notification if the event was sent by Coop
                    if !css.sent_ids.read().await.contains(&target.id) {
                        ingester.send(Signal::Message((target.id, event))).await;
                    }
                }
                // Old message: Coop is probably processing the user's messages during initial load
                false => {
                    css.gift_wrap_processing.store(true, Ordering::Release);
                }
            }
        }
    }

    fn process_registry_event(
        &mut self,
        event: &RegistryEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RegistryEvent::Open(room) => {
                if let Some(room) = room.upgrade() {
                    self.dock.update(cx, |this, cx| {
                        let panel = chat::init(room, window, cx);
                        this.add_panel(Arc::new(panel), DockPlacement::Center, window, cx);
                    });
                } else {
                    window.push_notification(t!("common.room_error"), cx);
                }
            }
            RegistryEvent::Close(..) => {
                self.dock.update(cx, |this, cx| {
                    this.focus_tab_panel(window, cx);

                    cx.defer_in(window, |_, window, cx| {
                        window.dispatch_action(Box::new(ClosePanel), cx);
                        window.close_all_modals(cx);
                    });
                });
            }
            _ => {}
        };
    }

    fn auth(&mut self, req: AuthRequest, window: &mut Window, cx: &mut Context<Self>) {
        let settings = AppSettings::global(cx);

        let challenge = req.challenge.to_owned();
        let url = req.url.to_owned();

        let challenge_clone = challenge.clone();
        let url_clone = url.clone();

        // Set Coop is sending auth for this request
        self.sending_auth_request(&challenge, cx);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let css = css();
            let signer = client.signer().await?;

            // Construct event
            let event: Event = EventBuilder::auth(challenge_clone, url_clone.clone())
                .sign(&signer)
                .await?;

            // Get the event ID
            let id = event.id;

            // Get the relay
            let relay = client.pool().relay(url_clone).await?;
            let relay_url = relay.url();

            // Subscribe to notifications
            let mut notifications = relay.notifications();

            // Send the AUTH message
            relay.send_msg(ClientMessage::Auth(Cow::Borrowed(&event)))?;

            while let Ok(notification) = notifications.recv().await {
                match notification {
                    RelayNotification::Message {
                        message: RelayMessage::Ok { event_id, .. },
                    } => {
                        if id == event_id {
                            // Re-subscribe to previous subscription
                            relay.resubscribe().await?;

                            // Get all failed events that need to be resent
                            let mut queue = css.resend_queue.write().await;

                            let ids: Vec<EventId> = queue
                                .iter()
                                .filter(|(_, url)| relay_url == *url)
                                .map(|(id, _)| *id)
                                .collect();

                            for id in ids.into_iter() {
                                if let Some(relay_url) = queue.remove(&id) {
                                    if let Some(event) = client.database().event_by_id(&id).await? {
                                        let event_id = relay.send_event(&event).await?;

                                        let output = Output {
                                            val: event_id,
                                            failed: HashMap::new(),
                                            success: HashSet::from([relay_url]),
                                        };

                                        css.sent_ids.write().await.insert(event_id);
                                        css.resent_ids.write().await.push(output);
                                    }
                                }
                            }

                            return Ok(());
                        }
                    }
                    RelayNotification::AuthenticationFailed => break,
                    RelayNotification::Shutdown => break,
                    _ => {}
                }
            }

            Err(anyhow!("Authentication failed"))
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(_) => {
                    this.update_in(cx, |this, window, cx| {
                        this.remove_auth_request(&challenge, cx);

                        // Save the authenticated relay to automatically authenticate future requests
                        settings.update(cx, |this, cx| {
                            this.push_relay(&url, cx);
                        });

                        // Clear the current notification
                        window.clear_notification_by_id(SharedString::from(challenge), cx);

                        // Push a new notification after current cycle
                        cx.defer_in(window, move |_, window, cx| {
                            window.push_notification(format!("{url} has been authenticated"), cx);
                        });
                    })
                    .ok();
                }
                Err(e) => {
                    this.update_in(cx, |_, window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn open_auth_request(&mut self, req: AuthRequest, window: &mut Window, cx: &mut Context<Self>) {
        let weak_view = cx.entity().downgrade();
        let challenge = req.challenge.to_owned();
        let relay_url = req.url.to_owned();
        let url_as_string = SharedString::from(relay_url.to_string());

        let note = Notification::new()
            .custom_id(SharedString::from(challenge.clone()))
            .autohide(false)
            .icon(IconName::Info)
            .title(t!("auth.label"))
            .content(move |_window, cx| {
                v_flex()
                    .gap_2()
                    .text_sm()
                    .child(shared_t!("auth.message"))
                    .child(
                        v_flex()
                            .py_1()
                            .px_1p5()
                            .rounded_sm()
                            .text_xs()
                            .bg(cx.theme().warning_background)
                            .text_color(cx.theme().warning_foreground)
                            .child(url_as_string.clone()),
                    )
                    .into_any_element()
            })
            .action(move |_window, cx| {
                let weak_view = weak_view.clone();
                let req = req.clone();
                let loading = weak_view
                    .read_with(cx, |this, cx| {
                        this.is_sending_auth_request(&req.challenge, cx)
                    })
                    .unwrap_or_default();

                Button::new("approve")
                    .label(t!("common.approve"))
                    .small()
                    .primary()
                    .loading(loading)
                    .disabled(loading)
                    .on_click(move |_e, window, cx| {
                        weak_view
                            .update(cx, |this, cx| {
                                this.auth(req.clone(), window, cx);
                            })
                            .ok();
                    })
            });

        window.push_notification(note, cx);
    }

    fn reopen_auth_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for req in self.auth_requests.clone().into_iter() {
            self.open_auth_request(req.0, window, cx);
        }
    }

    fn push_auth_request(&mut self, req: &AuthRequest, cx: &mut Context<Self>) {
        self.auth_requests.insert(req.to_owned(), false);
        cx.notify();
    }

    fn sending_auth_request(&mut self, challenge: &str, cx: &mut Context<Self>) {
        for (req, status) in self.auth_requests.iter_mut() {
            if req.challenge == challenge {
                *status = true;
                cx.notify();
            }
        }
    }

    fn is_sending_auth_request(&self, challenge: &str, _cx: &App) -> bool {
        if let Some(req) = self
            .auth_requests
            .iter()
            .find(|(req, _)| req.challenge == challenge)
        {
            req.1.to_owned()
        } else {
            false
        }
    }

    fn remove_auth_request(&mut self, challenge: &str, cx: &mut Context<Self>) {
        self.auth_requests.retain(|r, _| r.challenge != challenge);
        cx.notify();
    }

    fn set_onboarding_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = Arc::new(onboarding::init(window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn set_account_layout(
        &mut self,
        secret: String,
        profile: Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = Arc::new(account::init(secret, profile, window, cx));
        let center = DockItem::panel(panel);

        self.dock.update(cx, |this, cx| {
            this.reset(window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn set_default_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak_dock = self.dock.downgrade();

        let sidebar = Arc::new(sidebar::init(window, cx));
        let center = Arc::new(welcome::init(window, cx));

        let left = DockItem::panel(sidebar);
        let center = DockItem::split_with_sizes(
            Axis::Vertical,
            vec![DockItem::tabs(vec![center], None, &weak_dock, window, cx)],
            vec![None],
            &weak_dock,
            window,
            cx,
        );

        self.dock.update(cx, |this, cx| {
            this.set_left_dock(left, Some(px(DEFAULT_SIDEBAR_WIDTH)), true, window, cx);
            this.set_center(center, window, cx);
        });
    }

    fn set_no_nip17_relays(&mut self, cx: &mut Context<Self>) {
        self.has_nip17_relays = false;
        cx.notify();
    }

    fn load_local_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_IDENTIFIER)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                let metadata = client
                    .database()
                    .metadata(event.pubkey)
                    .await?
                    .unwrap_or_default();

                Ok((event.content, Profile::new(event.pubkey, metadata)))
            } else {
                Err(anyhow!("Empty"))
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok((secret, profile)) = task.await {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_account_layout(secret, profile, window, cx);
                    })
                    .ok();
                })
                .ok();
            } else {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_onboarding_layout(window, cx);
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();
    }

    fn on_settings(&mut self, _ev: &Settings, window: &mut Window, cx: &mut Context<Self>) {
        let view = preferences::init(window, cx);

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title(shared_t!("common.preferences"))
                .width(px(580.))
                .child(view.clone())
        });
    }

    fn on_dark_mode(&mut self, _ev: &DarkMode, window: &mut Window, cx: &mut Context<Self>) {
        if cx.theme().mode.is_dark() {
            Theme::change(ThemeMode::Light, Some(window), cx);
        } else {
            Theme::change(ThemeMode::Dark, Some(window), cx);
        }
    }

    fn on_reload_metadata(
        &mut self,
        _ev: &ReloadMetadata,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let css = css();

            let filter = Filter::new().kind(Kind::PrivateDirectMessage);

            let pubkeys: Vec<PublicKey> = client
                .database()
                .query(filter)
                .await?
                .into_iter()
                .flat_map(|event| event.all_pubkeys())
                .unique()
                .collect();

            let filter = Filter::new()
                .kind(Kind::Metadata)
                .limit(pubkeys.len())
                .authors(pubkeys);

            client
                .subscribe_to(BOOTSTRAP_RELAYS, filter, css.auto_close_opts)
                .await?;

            Ok(())
        });

        cx.spawn_in(window, async move |_, cx| {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    window.push_notification(t!("common.refreshed"), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn on_sign_out(&mut self, _e: &Logout, _window: &mut Window, cx: &mut Context<Self>) {
        cx.background_spawn(async move {
            let client = nostr_client();
            let ingester = ingester();

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .identifier(ACCOUNT_IDENTIFIER);

            // Delete account
            client.database().delete(filter).await.ok();

            // Reset the nostr client
            client.reset().await;

            // Notify the channel about the signer being unset
            ingester.send(Signal::SignerUnset).await;
        })
        .detach();
    }

    fn on_open_profile(&mut self, ev: &OpenProfile, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = ev.0;
        let profile = user_profile::init(public_key, window, cx);

        window.open_modal(cx, move |this, _window, _cx| {
            this.alert()
                .show_close(true)
                .overlay_closable(true)
                .child(profile.clone())
                .button_props(ModalButtonProps::default().ok_text(t!("profile.njump")))
                .on_ok(move |_, _window, cx| {
                    let Ok(bech32) = public_key.to_bech32();
                    cx.open_url(&format!("https://njump.me/{bech32}"));
                    false
                })
        });
    }

    fn render_proxy_modal(&mut self, window: &mut Window, cx: &mut App) {
        window.open_modal(cx, |this, _window, _cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .alert()
                .button_props(ModalButtonProps::default().ok_text(t!("common.open_browser")))
                .title(shared_t!("proxy.label"))
                .child(
                    v_flex()
                        .p_3()
                        .gap_1()
                        .w_full()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_sm()
                        .child(shared_t!("proxy.description")),
                )
                .on_ok(move |_e, _window, cx| {
                    cx.open_url("http://localhost:7400");
                    false
                })
        });
    }

    fn render_client_keys_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .button_props(
                    ModalButtonProps::default()
                        .cancel_text(t!("startup.create_new_keys"))
                        .ok_text(t!("common.allow")),
                )
                .child(
                    div()
                        .w_full()
                        .h_40()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_sm()
                        .child(
                            div()
                                .font_semibold()
                                .text_color(cx.theme().text_muted)
                                .child(shared_t!("startup.client_keys_warning")),
                        )
                        .child(shared_t!("startup.client_keys_desc")),
                )
                .on_cancel(|_, _window, cx| {
                    ClientKeys::global(cx).update(cx, |this, cx| {
                        this.new_keys(cx);
                    });
                    // true: Close modal
                    true
                })
                .on_ok(|_, window, cx| {
                    ClientKeys::global(cx).update(cx, |this, cx| {
                        this.load(window, cx);
                    });
                    // true: Close modal
                    true
                })
        });
    }

    fn render_titlebar_left_side(
        &mut self,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let registry = Registry::read_global(cx);
        let status = registry.unwrapping_status.read(cx);

        h_flex()
            .gap_2()
            .h_6()
            .w_full()
            .child(compose_button())
            .when(status != &UnwrappingStatus::Complete, |this| {
                this.child(
                    h_flex()
                        .px_2()
                        .h_6()
                        .gap_1()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().surface_background)
                        .child(shared_t!("loading.label")),
                )
            })
    }

    fn render_titlebar_right_side(
        &mut self,
        profile: &Profile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let is_auto_auth = AppSettings::read_global(cx).is_auto_auth();
        let updating = AutoUpdater::read_global(cx).status.is_updating();
        let updated = AutoUpdater::read_global(cx).status.is_updated();
        let auth_requests = self.auth_requests.len();

        h_flex()
            .gap_1()
            .when(updating, |this| {
                this.child(
                    h_flex()
                        .h_6()
                        .px_2()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().ghost_element_background_alt)
                        .child(shared_t!("auto_update.updating")),
                )
            })
            .when(updated, |this| {
                this.child(
                    h_flex()
                        .id("updated")
                        .h_6()
                        .px_2()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().ghost_element_background_alt)
                        .hover(|this| this.bg(cx.theme().ghost_element_hover))
                        .active(|this| this.bg(cx.theme().ghost_element_active))
                        .child(shared_t!("auto_update.updated"))
                        .on_click(|_, _window, cx| {
                            cx.restart();
                        }),
                )
            })
            .when(auth_requests > 0 && !is_auto_auth, |this| {
                this.child(
                    h_flex()
                        .id("requests")
                        .h_6()
                        .px_2()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().warning_background)
                        .text_color(cx.theme().warning_foreground)
                        .hover(|this| this.bg(cx.theme().warning_hover))
                        .active(|this| this.bg(cx.theme().warning_active))
                        .child(shared_t!("auth.requests", u = auth_requests))
                        .on_click(cx.listener(move |this, _e, window, cx| {
                            this.reopen_auth_request(window, cx);
                        })),
                )
            })
            .when(!self.has_nip17_relays, |this| {
                this.child(setup_nip17_relay(t!("relays.button")))
            })
            .child(
                Button::new("user")
                    .small()
                    .reverse()
                    .transparent()
                    .icon(IconName::CaretDown)
                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(1.49)))
                    .popup_menu(|this, _window, _cx| {
                        this.menu(t!("user.dark_mode"), Box::new(DarkMode))
                            .menu(t!("user.settings"), Box::new(Settings))
                            .separator()
                            .menu(t!("user.reload_metadata"), Box::new(ReloadMetadata))
                            .separator()
                            .menu(t!("user.sign_out"), Box::new(Logout))
                    }),
            )
    }

    pub(crate) fn proxy_signer(window: &mut Window, cx: &mut App) {
        let Some(Some(root)) = window.root::<Root>() else {
            return;
        };

        let Ok(chatspace) = root.read(cx).view().clone().downcast::<ChatSpace>() else {
            return;
        };

        chatspace.update(cx, |this, cx| {
            let proxy = BrowserSignerProxy::new(BrowserSignerProxyOptions::default());
            let url = proxy.url();

            this._tasks.push(cx.background_spawn(async move {
                let client = nostr_client();
                let ingester = ingester();

                if proxy.start().await.is_ok() {
                    webbrowser::open(&url).ok();

                    loop {
                        if proxy.is_session_active() {
                            // Save the signer to disk for further logins
                            if let Ok(public_key) = proxy.get_public_key().await {
                                let keys = Keys::generate();
                                let tags = vec![Tag::identifier(ACCOUNT_IDENTIFIER)];
                                let kind = Kind::ApplicationSpecificData;

                                let builder = EventBuilder::new(kind, "extension")
                                    .tags(tags)
                                    .build(public_key)
                                    .sign(&keys)
                                    .await;

                                if let Ok(event) = builder {
                                    if let Err(e) = client.database().save_event(&event).await {
                                        log::error!("Failed to save event: {e}");
                                    };
                                }
                            }

                            // Set the client's signer with current proxy signer
                            client.set_signer(proxy.clone()).await;

                            break;
                        } else {
                            ingester.send(Signal::ProxyDown).await;
                        }
                        smol::Timer::after(Duration::from_secs(1)).await;
                    }
                }
            }));
        });
    }

    fn get_all_panel_ids(&self, cx: &App) -> Option<Vec<u64>> {
        let ids: Vec<u64> = self
            .dock
            .read(cx)
            .items
            .panel_ids(cx)
            .into_iter()
            .filter_map(|panel| panel.parse::<u64>().ok())
            .collect();

        Some(ids)
    }

    pub(crate) fn set_center_panel<P>(panel: P, window: &mut Window, cx: &mut App)
    where
        P: PanelView,
    {
        if let Some(Some(root)) = window.root::<Root>() {
            if let Ok(chatspace) = root.read(cx).view().clone().downcast::<ChatSpace>() {
                let panel = Arc::new(panel);
                let center = DockItem::panel(panel);

                chatspace.update(cx, |this, cx| {
                    this.dock.update(cx, |this, cx| {
                        this.set_center(center, window, cx);
                    });
                });
            }
        }
    }
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let registry = Registry::read_global(cx);

        // Only render titlebar child elements if user is logged in
        if registry.identity.is_some() {
            let profile = registry.identity(cx);

            let left_side = self
                .render_titlebar_left_side(window, cx)
                .into_any_element();

            let right_side = self
                .render_titlebar_right_side(&profile, window, cx)
                .into_any_element();

            self.title_bar.update(cx, |this, _cx| {
                this.set_children(vec![left_side, right_side]);
            })
        }

        div()
            .on_action(cx.listener(Self::on_settings))
            .on_action(cx.listener(Self::on_dark_mode))
            .on_action(cx.listener(Self::on_sign_out))
            .on_action(cx.listener(Self::on_open_profile))
            .on_action(cx.listener(Self::on_reload_metadata))
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    // Title Bar
                    .child(self.title_bar.clone())
                    // Dock
                    .child(self.dock.clone()),
            )
            // Notifications
            .children(notification_layer)
            // Modals
            .children(modal_layer)
    }
}
