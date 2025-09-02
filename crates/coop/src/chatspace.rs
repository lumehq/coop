use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Error};
use auto_update::AutoUpdater;
use client_keys::ClientKeys;
use common::display::ReadableProfile;
use common::event::EventUtils;
use global::constants::{
    ACCOUNT_IDENTIFIER, BOOTSTRAP_RELAYS, DEFAULT_SIDEBAR_WIDTH, METADATA_BATCH_LIMIT,
    METADATA_BATCH_TIMEOUT, RELAY_RETRY, SEARCH_RELAYS, WAIT_FOR_FINISH,
};
use global::{css, ingester, nostr_client, AuthRequest, IngesterSignal, Notice};
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
use smol::channel::{Receiver, Sender};
use smol::lock::Mutex;
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
use ui::tooltip::Tooltip;
use ui::{h_flex, v_flex, ContextModal, IconName, Root, Sizable, StyledExt};

use crate::actions::{DarkMode, Logout, Settings};
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

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
enum RelayTrackStatus {
    #[default]
    Waiting,
    NotFound,
    Found,
}

#[derive(Debug, Clone, Default)]
struct RelayTracking {
    nip17: RelayTrackStatus,
    nip65: RelayTrackStatus,
}

pub struct ChatSpace {
    title_bar: Entity<TitleBar>,
    dock: Entity<DockArea>,
    auth_requests: Vec<(String, RelayUrl)>,
    has_nip17_relays: bool,
    _subscriptions: SmallVec<[Subscription; 2]>,
    _tasks: SmallVec<[Task<()>; 3]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = Registry::global(cx);
        let client_keys = ClientKeys::global(cx);

        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| DockArea::new(window, cx));

        let relay_tracking = Arc::new(Mutex::new(RelayTracking::default()));
        let relay_tracking_clone = relay_tracking.clone();

        let (pubkey_tx, pubkey_rx) = smol::channel::bounded::<PublicKey>(1024);
        let (event_tx, event_rx) = smol::channel::bounded::<Event>(2048);

        let pubkey_tx_clone = pubkey_tx.clone();

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

                Self::process_nostr_events(&relay_tracking_clone, &event_tx, &pubkey_tx_clone)
                    .await
                    .expect("Failed to handle nostr events. Please restart the application.");
            }),
        );

        tasks.push(
            // Wait for the signer to be set
            // Also verify NIP65 and NIP17 relays after the signer is set
            cx.background_spawn(async move {
                Self::wait_for_signer_set(&relay_tracking).await;
            }),
        );

        tasks.push(
            // Listen all metadata requests then batch them into single subscription
            cx.background_spawn(async move {
                Self::process_batching_metadata(&pubkey_rx).await;
            }),
        );

        tasks.push(
            // Process gift wrap event in the background
            cx.background_spawn(async move {
                Self::process_gift_wrap(&pubkey_tx, &event_rx).await;
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
            auth_requests: vec![],
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

    async fn wait_for_signer_set(relay_tracking: &Arc<Mutex<RelayTracking>>) {
        let client = nostr_client();
        let ingester = ingester();

        let mut signer_set = false;
        let mut retry = 0;
        let mut nip65_retry = 0;

        loop {
            if signer_set {
                let state = relay_tracking.lock().await;

                if state.nip65 == RelayTrackStatus::Found {
                    if state.nip17 == RelayTrackStatus::Found {
                        break;
                    } else if state.nip17 == RelayTrackStatus::NotFound {
                        ingester.send(IngesterSignal::DmRelayNotFound).await;
                        break;
                    } else {
                        retry += 1;
                        if retry == RELAY_RETRY {
                            ingester.send(IngesterSignal::DmRelayNotFound).await;
                            break;
                        }
                    }
                } else {
                    nip65_retry += 1;
                    if nip65_retry == RELAY_RETRY {
                        ingester.send(IngesterSignal::DmRelayNotFound).await;
                        break;
                    }
                }
            }

            if !signer_set {
                if let Ok(signer) = client.signer().await {
                    if let Ok(public_key) = signer.get_public_key().await {
                        signer_set = true;

                        // Notify the app that the signer has been set.
                        ingester.send(IngesterSignal::SignerSet(public_key)).await;

                        // Subscribe to the NIP-65 relays for the public key.
                        if let Err(e) = Self::fetch_nip65_relays(public_key).await {
                            log::error!("Failed to fetch NIP-65 relays: {e}");
                        }
                    }
                }
            }

            smol::Timer::after(Duration::from_millis(300)).await;
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
            match smol::future::or(
                async {
                    if let Ok(public_key) = rx.recv().await {
                        BatchEvent::PublicKey(public_key)
                    } else {
                        BatchEvent::Closed
                    }
                },
                async {
                    smol::Timer::after(timeout).await;
                    BatchEvent::Timeout
                },
            )
            .await
            {
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
                    // Exit the current loop
                    break;
                }
            }
        }
    }

    async fn process_gift_wrap(pubkey_tx: &Sender<PublicKey>, event_rx: &Receiver<Event>) {
        let client = nostr_client();
        let ingester = ingester();
        let timeout = Duration::from_secs(WAIT_FOR_FINISH);

        let mut counter = 0;

        loop {
            // Signer is unset, probably user is not ready to retrieve gift wrap events
            if client.signer().await.is_err() {
                smol::Timer::after(Duration::from_secs(1)).await;
                continue;
            }

            let recv = || async {
                // no inline
                (event_rx.recv().await).ok()
            };

            let timeout = || async {
                smol::Timer::after(timeout).await;
                None
            };

            match smol::future::or(recv(), timeout()).await {
                Some(event) => {
                    let cached = Self::unwrap_gift_wrap_event(&event, pubkey_tx).await;

                    // Increment the total messages counter if message is not from cache
                    if !cached {
                        counter += 1;
                    }

                    // Send partial finish signal to GPUI
                    if counter >= 20 {
                        ingester.send(IngesterSignal::PartialFinish).await;
                        // Reset counter
                        counter = 0;
                    }
                }
                None => {
                    // Notify the UI that the processing is finished
                    ingester.send(IngesterSignal::Finish).await;
                    break;
                }
            }
        }
    }

    async fn process_nostr_events(
        relay_tracking: &Arc<Mutex<RelayTracking>>,
        event_tx: &Sender<Event>,
        pubkey_tx: &Sender<PublicKey>,
    ) -> Result<(), Error> {
        let client = nostr_client();
        let ingester = ingester();
        let css = css();
        let auto_close =
            SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

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
                            // Get metadata for event's pubkey that matches the current user's pubkey
                            if let Ok(true) = Self::is_self_event(&event).await {
                                let mut relay_tracking = relay_tracking.lock().await;
                                relay_tracking.nip65 = RelayTrackStatus::Found;

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
                                let relays: Vec<RelayUrl> = event
                                    .tags
                                    .filter_standardized(TagKind::Relay)
                                    .filter_map(|t| {
                                        if let TagStandard::Relay(url) = t {
                                            Some(url.to_owned())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                if !relays.is_empty() {
                                    let mut relay_tracking = relay_tracking.lock().await;
                                    relay_tracking.nip17 = RelayTrackStatus::Found;

                                    for relay in relays.iter() {
                                        if client.add_relay(relay).await.is_err() {
                                            let notice = Notice::RelayFailed(relay.clone());
                                            ingester.send(IngesterSignal::Notice(notice)).await;
                                        }
                                        if client.connect_relay(relay).await.is_err() {
                                            let notice = Notice::RelayFailed(relay.clone());
                                            ingester.send(IngesterSignal::Notice(notice)).await;
                                        }
                                    }

                                    // Subscribe to gift wrap events only in the current user's NIP-17 relays
                                    Self::fetch_gift_wrap(&relays, event.pubkey).await;
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
                                    .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(auto_close))
                                    .await
                                    .ok();
                            }
                        }
                        Kind::Metadata => {
                            ingester
                                .send(IngesterSignal::Metadata(event.into_owned()))
                                .await;
                        }
                        Kind::GiftWrap => {
                            if event_tx.send(event.clone().into_owned()).await.is_err() {
                                Self::unwrap_gift_wrap_event(&event, pubkey_tx).await;
                            }
                        }
                        _ => {}
                    }
                }
                RelayMessage::Auth { challenge } => {
                    if challenges.insert(challenge.clone()) {
                        let req = AuthRequest::new(challenge, relay_url);
                        // Send a signal to the ingester to handle the auth request
                        ingester.send(IngesterSignal::Auth(req)).await;
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
                            if client.has_signer().await {
                                css.resend_queue.write().await.insert(event_id, relay_url);
                            };
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

        while let Ok(signal) = signals.recv().await {
            cx.update(|window, cx| {
                let registry = Registry::global(cx);
                let settings = AppSettings::global(cx);

                match signal {
                    IngesterSignal::SignerSet(public_key) => {
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
                    IngesterSignal::SignerUnset => {
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
                    IngesterSignal::Auth(req) => {
                        let relay_url = &req.url;
                        let challenge = &req.challenge;
                        let auto_auth = AppSettings::get_auto_auth(cx);
                        let is_authenticated_relays =
                            AppSettings::read_global(cx).is_authenticated_relays(relay_url);

                        view.update(cx, |this, cx| {
                            this.push_auth_request(challenge, relay_url, cx);

                            if auto_auth && is_authenticated_relays {
                                // Automatically authenticate if the relay is authenticated before
                                this.auth(challenge, relay_url, window, cx);
                            } else {
                                // Otherwise open the auth request popup
                                this.open_auth_request(challenge, relay_url, window, cx);
                            }
                        })
                        .ok();
                    }
                    IngesterSignal::ProxyDown => {
                        if !is_open_proxy_modal {
                            view.update(cx, |this, cx| {
                                this.render_proxy_modal(window, cx);
                            })
                            .ok();
                            is_open_proxy_modal = true;
                        }
                    }
                    // Load chat rooms and stop the loading status
                    IngesterSignal::Finish => {
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                            this.set_loading(false, cx);
                            // Send a signal to refresh all opened rooms' messages
                            if let Some(ids) = ChatSpace::all_panels(window, cx) {
                                this.refresh_rooms(ids, cx);
                            }
                        });
                    }
                    // Load chat rooms without setting as finished
                    IngesterSignal::PartialFinish => {
                        registry.update(cx, |this, cx| {
                            this.load_rooms(window, cx);
                            // Send a signal to refresh all opened rooms' messages
                            if let Some(ids) = ChatSpace::all_panels(window, cx) {
                                this.refresh_rooms(ids, cx);
                            }
                        });
                    }
                    // Add the new metadata to the registry or update the existing one
                    IngesterSignal::Metadata(event) => {
                        registry.update(cx, |this, cx| {
                            this.insert_or_update_person(event, cx);
                        });
                    }
                    // Convert the gift wrapped message to a message
                    IngesterSignal::GiftWrap((gift_wrap_id, event)) => {
                        registry.update(cx, |this, cx| {
                            this.event_to_message(gift_wrap_id, event, window, cx);
                        });
                    }
                    IngesterSignal::DmRelayNotFound => {
                        view.update(cx, |this, cx| {
                            this.set_no_nip17_relays(cx);
                        })
                        .ok();
                    }
                    IngesterSignal::Notice(msg) => {
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
    async fn fetch_single_event(kind: Kind, public_key: PublicKey) {
        let client = nostr_client();
        let auto_close =
            SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let filter = Filter::new().kind(kind).author(public_key).limit(1);

        if let Err(e) = client.subscribe(filter, Some(auto_close)).await {
            log::info!("Failed to subscribe: {e}");
        }
    }

    async fn fetch_gift_wrap(relays: &[RelayUrl], public_key: PublicKey) {
        let client = nostr_client();
        let subscription_id = SubscriptionId::new("inbox");
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);

        if client
            .subscribe_with_id_to(relays.to_owned(), subscription_id, filter, None)
            .await
            .is_ok()
        {
            log::info!("Subscribed to messages in: {relays:?}");
        }
    }

    /// Fetches NIP-65 relay list for a given public key
    async fn fetch_nip65_relays(public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let auto_close =
            SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(auto_close))
            .await?;

        Ok(())
    }

    /// Fetches metadata for a list of public keys
    async fn fetch_metadata_for_pubkeys(public_keys: HashSet<PublicKey>) {
        if public_keys.is_empty() {
            return;
        };

        let client = nostr_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList];
        let limit = public_keys.len() * kinds.len();
        let filter = Filter::new().limit(limit).authors(public_keys).kinds(kinds);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
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
    async fn unwrap_gift_wrap_event(gift: &Event, pubkey_tx: &Sender<PublicKey>) -> bool {
        let client = nostr_client();
        let ingester = ingester();
        let css = css();
        let mut is_cached = false;

        let event = match Self::get_unwrapped_event(gift.id).await {
            Ok(event) => {
                is_cached = true;
                event
            }
            Err(_) => {
                match client.unwrap_gift_wrap(gift).await {
                    Ok(unwrap) => {
                        // Sign the unwrapped event with a RANDOM KEYS
                        let Ok(unwrapped) = unwrap.rumor.sign_with_keys(&Keys::generate()) else {
                            log::error!("Failed to sign event");
                            return false;
                        };

                        // Save this event to the database for future use.
                        if let Err(e) = Self::set_unwrapped_event(gift.id, &unwrapped).await {
                            log::warn!("Failed to cache unwrapped event: {e}")
                        }

                        unwrapped
                    }
                    Err(e) => {
                        log::error!("Failed to unwrap event: {e}");
                        return false;
                    }
                }
            }
        };

        // Send all pubkeys to the metadata batch to sync data
        for public_key in event.all_pubkeys() {
            pubkey_tx.send(public_key).await.ok();
        }

        // Send a notify to GPUI if this is a new message
        if event.created_at >= css.init_at {
            ingester
                .send(IngesterSignal::GiftWrap((gift.id, event)))
                .await;
        }

        is_cached
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

    fn auth(
        &mut self,
        challenge: &str,
        url: &RelayUrl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = AppSettings::global(cx);
        let challenge = challenge.to_string();
        let url = url.to_owned();
        let challenge_clone = challenge.clone();
        let url_clone = url.clone();

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
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.remove_auth_request(&challenge, cx);

                            // Save the authenticated relay to automatically authenticate future requests
                            settings.update(cx, |this, cx| {
                                this.push_relay(&url, cx);
                            });

                            // Clear the current notification
                            window.clear_notification_by_id(SharedString::from(challenge), cx);

                            // Push a new notification after current cycle
                            cx.defer_in(window, move |_, window, cx| {
                                window
                                    .push_notification(format!("{url} has been authenticated"), cx);
                            });
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn open_auth_request(
        &mut self,
        challenge: &str,
        relay_url: &RelayUrl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak_view = cx.entity().downgrade();
        let challenge = challenge.to_string();
        let relay_url = relay_url.to_owned();
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
            .action(move |_window, _cx| {
                let weak_view = weak_view.clone();
                let challenge = challenge.clone();
                let relay_url = relay_url.clone();

                Button::new("approve")
                    .label(t!("common.approve"))
                    .small()
                    .primary()
                    .on_click(move |_e, window, cx| {
                        weak_view
                            .update(cx, |this, cx| {
                                this.auth(&challenge, &relay_url, window, cx);
                            })
                            .ok();
                    })
            });

        window.push_notification(note, cx);
    }

    fn reopen_auth_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (challenge, relay_url) in self.auth_requests.clone().iter() {
            self.open_auth_request(challenge, relay_url, window, cx);
        }
    }

    fn push_auth_request(&mut self, challenge: &str, url: &RelayUrl, cx: &mut Context<Self>) {
        self.auth_requests.push((challenge.into(), url.to_owned()));
        cx.notify();
    }

    fn remove_auth_request(&mut self, challenge: &str, cx: &mut Context<Self>) {
        if let Some(ix) = self.auth_requests.iter().position(|(c, _)| c == challenge) {
            self.auth_requests.remove(ix);
            cx.notify();
        }
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

    fn set_no_nip17_relays(&mut self, cx: &mut Context<Self>) {
        self.has_nip17_relays = false;
        cx.notify();
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
            ingester.send(IngesterSignal::SignerUnset).await;
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
        let loading = self.has_nip17_relays && self.auth_requests.is_empty() && registry.loading;

        h_flex()
            .gap_2()
            .w_full()
            .child(compose_button())
            .when(loading, |this| {
                this.child(
                    h_flex()
                        .id("downloading")
                        .px_4()
                        .h_6()
                        .gap_1()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().surface_background)
                        .child(shared_t!("loading.label"))
                        .tooltip(|window, cx| {
                            Tooltip::new(t!("loading.tooltip"), window, cx).into()
                        }),
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
                this.child(setup_nip17_relay(t!("relays.button_label")))
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
                            ingester.send(IngesterSignal::ProxyDown).await;
                        }
                        smol::Timer::after(Duration::from_secs(1)).await;
                    }
                }
            }));
        });
    }

    pub(crate) fn all_panels(window: &mut Window, cx: &mut App) -> Option<Vec<u64>> {
        let Some(Some(root)) = window.root::<Root>() else {
            return None;
        };

        let Ok(chatspace) = root.read(cx).view().clone().downcast::<ChatSpace>() else {
            return None;
        };

        let ids: Vec<u64> = chatspace
            .read(cx)
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
            let profile = Registry::read_global(cx).identity(cx);

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
            .relative()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
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
