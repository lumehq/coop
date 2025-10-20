use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Error};
use auto_update::AutoUpdater;
use common::display::RenderedProfile;
use common::event::EventUtils;
use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, relative, rems, App, AppContext, AsyncWindowContext, Axis, ClipboardItem,
    Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, WeakEntity, Window,
};
use i18n::{shared_t, t};
use itertools::Itertools;
use nostr_connect::prelude::*;
use nostr_sdk::prelude::*;
use registry::keystore::KeyItem;
use registry::{Registry, RegistryEvent};
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use states::constants::{BOOTSTRAP_RELAYS, DEFAULT_SIDEBAR_WIDTH};
use states::state::{AuthRequest, SignalKind, UnwrappingStatus};
use states::{app_state, default_nip17_relays, default_nip65_relays};
use theme::{ActiveTheme, Theme, ThemeMode};
use title_bar::TitleBar;
use ui::actions::{CopyPublicKey, OpenPublicKey};
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::dock::DockPlacement;
use ui::dock_area::panel::PanelView;
use ui::dock_area::{ClosePanel, DockArea, DockItem};
use ui::modal::ModalButtonProps;
use ui::notification::Notification;
use ui::popup_menu::PopupMenuExt;
use ui::{h_flex, v_flex, ContextModal, Disableable, IconName, Root, Sizable, StyledExt};

use crate::actions::{reset, DarkMode, Logout, ReloadMetadata, Settings};
use crate::views::compose::compose_button;
use crate::views::setup_relay::SetupRelay;
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
    /// App's Title Bar
    title_bar: Entity<TitleBar>,

    /// App's Dock Area
    dock: Entity<DockArea>,

    /// All authentication requests
    auth_requests: Entity<HashMap<RelayUrl, AuthRequest>>,

    /// Local state to determine if the user has set up NIP-17 relays
    nip17_ready: bool,

    /// Local state to determine if the user has set up NIP-65 relays
    nip65_ready: bool,

    /// All subscriptions for observing the app state
    _subscriptions: SmallVec<[Subscription; 4]>,

    /// All long running tasks
    _tasks: SmallVec<[Task<()>; 5]>,
}

impl ChatSpace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = Registry::global(cx);
        let status = registry.read(cx).unwrapping_status.clone();

        let title_bar = cx.new(|_| TitleBar::new());
        let dock = cx.new(|cx| DockArea::new(window, cx));
        let auth_requests = cx.new(|_| HashMap::new());

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Automatically sync theme with system appearance
            window.observe_window_appearance(|window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
            }),
        );

        subscriptions.push(
            // Observe the keystore
            cx.observe_in(&registry, window, |this, registry, window, cx| {
                let has_keyring = registry.read(cx).initialized_keystore;
                let use_filestore = registry.read(cx).is_using_file_keystore();
                let not_logged_in = registry.read(cx).signer_pubkey().is_none();

                if use_filestore && not_logged_in {
                    this.render_keyring_installation(window, cx);
                }

                if has_keyring && not_logged_in {
                    let keystore = registry.read(cx).keystore();

                    cx.spawn_in(window, async move |this, cx| {
                        let result = keystore
                            .read_credentials(&KeyItem::User.to_string(), cx)
                            .await;

                        this.update_in(cx, |this, window, cx| {
                            match result {
                                Ok(Some((user, secret))) => {
                                    let public_key = PublicKey::parse(&user).unwrap();
                                    let secret = String::from_utf8(secret).unwrap();
                                    this.set_account_layout(public_key, secret, window, cx);
                                }
                                _ => {
                                    this.set_onboarding_layout(window, cx);
                                }
                            };
                        })
                        .ok();
                    })
                    .detach();
                }
            }),
        );

        subscriptions.push(
            // Observe the global registry's events
            cx.observe_in(&status, window, move |this, status, window, cx| {
                let status = status.read(cx);
                let all_panels = this.get_all_panel_ids(cx);

                if matches!(
                    status,
                    UnwrappingStatus::Processing | UnwrappingStatus::Complete
                ) {
                    Registry::global(cx).update(cx, |this, cx| {
                        this.load_rooms(window, cx);
                        this.refresh_rooms(all_panels, cx);
                    });
                }
            }),
        );

        subscriptions.push(
            // Handle registry events
            cx.subscribe_in(&registry, window, move |this, _, ev, window, cx| {
                match ev {
                    RegistryEvent::Open(room) => {
                        if let Some(room) = room.upgrade() {
                            this.dock.update(cx, |this, cx| {
                                let panel = chat::init(room, window, cx);
                                this.add_panel(Arc::new(panel), DockPlacement::Center, window, cx);
                            });
                        }
                    }
                    RegistryEvent::Close(..) => {
                        this.dock.update(cx, |this, cx| {
                            this.focus_tab_panel(window, cx);

                            cx.defer_in(window, |_, window, cx| {
                                window.dispatch_action(Box::new(ClosePanel), cx);
                                window.close_all_modals(cx);
                            });
                        });
                    }
                    _ => {}
                };
            }),
        );

        tasks.push(
            // Handle nostr events in the background
            cx.background_spawn(async move {
                app_state().handle_notifications().await.ok();
            }),
        );

        tasks.push(
            // Listen all metadata requests then batch them into single subscription
            cx.background_spawn(async move {
                app_state().handle_metadata_batching().await;
            }),
        );

        tasks.push(
            // Wait for the signer to be set
            // Also verify NIP-65 and NIP-17 relays after the signer is set
            cx.background_spawn(async move {
                app_state().observe_signer().await;
            }),
        );

        tasks.push(
            // Observe gift wrap process in the background
            cx.background_spawn(async move {
                app_state().observe_giftwrap().await;
            }),
        );

        tasks.push(
            // Continuously handle signals from the Nostr channel
            cx.spawn_in(window, async move |this, cx| {
                Self::handle_signals(this, cx).await
            }),
        );

        Self {
            dock,
            title_bar,
            auth_requests,
            nip17_ready: true,
            nip65_ready: true,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn handle_signals(view: WeakEntity<ChatSpace>, cx: &mut AsyncWindowContext) {
        let states = app_state();

        while let Ok(signal) = states.signal().receiver().recv_async().await {
            view.update_in(cx, |this, window, cx| {
                let registry = Registry::global(cx);
                let settings = AppSettings::global(cx);

                match signal {
                    SignalKind::EncryptionNotSet => {
                        this.new_encryption(window, cx);
                    }
                    SignalKind::EncryptionSet(n) => {
                        this.reinit_encryption(n, window, cx);
                    }
                    SignalKind::SignerSet(public_key) => {
                        // Close the latest modal if it exists
                        window.close_modal(cx);

                        // Load user's settings
                        settings.update(cx, |this, cx| {
                            this.load_settings(cx);
                        });

                        // Load all chat rooms
                        registry.update(cx, |this, cx| {
                            this.set_signer_pubkey(public_key, cx);
                            this.load_client_keys(cx);
                            this.load_rooms(window, cx);
                        });

                        // Setup the default layout for current workspace
                        this.set_default_layout(window, cx);
                    }
                    SignalKind::Auth(req) => {
                        let url = &req.url;
                        let auto_auth = AppSettings::get_auto_auth(cx);
                        let is_authenticated = AppSettings::read_global(cx).is_authenticated(url);

                        // Store the auth request in the current view
                        this.push_auth_request(&req, cx);

                        if auto_auth && is_authenticated {
                            // Automatically authenticate if the relay is authenticated before
                            this.auth(req, window, cx);
                        } else {
                            // Otherwise open the auth request popup
                            this.open_auth_request(req, window, cx);
                        }
                    }
                    SignalKind::GiftWrapStatus(status) => {
                        registry.update(cx, |this, cx| {
                            this.set_unwrapping_status(status, cx);
                        });
                    }
                    SignalKind::NewProfile(profile) => {
                        registry.update(cx, |this, cx| {
                            this.insert_or_update_person(profile, cx);
                        });
                    }
                    SignalKind::NewMessage((gift_wrap_id, event)) => {
                        registry.update(cx, |this, cx| {
                            this.event_to_message(gift_wrap_id, event, window, cx);
                        });
                    }
                    SignalKind::GossipRelaysNotFound => {
                        this.set_required_gossip_relays(cx);
                        this.render_setup_gossip_relays_modal(window, cx);
                    }
                    SignalKind::MessagingRelaysNotFound => {
                        this.set_required_dm_relays(cx);
                    }
                };
            })
            .ok();
        }
    }

    fn new_encryption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = Keys::generate();
        let username = keys.public_key().to_hex();
        let password = keys.secret_key().to_secret_bytes();

        let keystore = Registry::global(cx).read(cx).keystore();
        let url = KeyItem::Encryption;

        cx.spawn_in(window, async move |this, cx| {
            let result = keystore
                .write_credentials(&url.to_string(), &username, &password, cx)
                .await;

            this.update_in(cx, |_this, window, cx| {
                match result {
                    Ok(_) => {
                        Registry::global(cx).update(cx, |this, cx| {
                            this.set_encryption_keys(keys, cx);
                        });
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn reinit_encryption(&mut self, n: PublicKey, window: &mut Window, cx: &mut Context<Self>) {
        let keystore = Registry::global(cx).read(cx).keystore();
        let url = KeyItem::Encryption;

        cx.spawn_in(window, async move |this, cx| {
            let result = keystore.read_credentials(&url.to_string(), cx).await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Some((username, password))) => {
                        let public_key = PublicKey::from_hex(&username).unwrap();

                        if n == public_key {
                            let secret = SecretKey::from_slice(&password).unwrap();
                            let keys = Keys::new(secret);

                            Registry::global(cx).update(cx, |this, cx| {
                                this.set_encryption_keys(keys, cx);
                            });
                        } else {
                            this.request_encryption(window, cx);
                        }
                    }
                    Ok(None) => {
                        //
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn request_encryption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let registry = Registry::global(cx).read(cx);

        let Some(client_keys) = registry.client_keys.read(cx).clone() else {
            window.push_notification("Client Keys is required", cx);
            return;
        };

        let get_local_response: Task<Result<Option<Keys>, Error>> =
            cx.background_spawn(async move {
                let client = app_state().client();
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;

                let filter = Filter::new()
                    .author(public_key)
                    .kind(Kind::Custom(4455))
                    .limit(1);

                if let Some(event) = client.database().query(filter).await?.first_owned() {
                    if let Some(target) = event
                        .tags
                        .find(TagKind::custom("P"))
                        .and_then(|tag| tag.content())
                        .and_then(|content| PublicKey::parse(content).ok())
                    {
                        let decrypted = client_keys.nip44_decrypt(&target, &event.content).await?;
                        let secret = SecretKey::from_hex(&decrypted)?;
                        let keys = Keys::new(secret);

                        return Ok(Some(keys));
                    }
                }

                Ok(None)
            });

        let Some(client_keys) = registry.client_keys.read(cx).clone() else {
            window.push_notification("Client Keys is required", cx);
            return;
        };

        let send_new_request: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = app_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let client_pubkey = client_keys.get_public_key().await?;

            let event = EventBuilder::new(Kind::Custom(4454), "")
                .tags(vec![
                    Tag::custom(TagKind::custom("P"), vec![client_pubkey]),
                    Tag::public_key(public_key),
                ])
                .sign(&signer)
                .await?;

            client.send_event(&event).await?;

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            match get_local_response.await {
                Ok(Some(keys)) => {
                    this.update(cx, |_this, cx| {
                        Registry::global(cx).update(cx, |this, cx| {
                            this.set_encryption_keys(keys, cx);
                        });
                    })
                    .ok();
                }
                _ => {
                    send_new_request.await.ok();
                }
            };
        })
        .detach();
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
            let states = app_state();
            let client = states.client();
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
                            let mut tracker = states.tracker().write().await;

                            let ids: Vec<EventId> = tracker
                                .resend_queue
                                .iter()
                                .filter(|(_, url)| relay_url == *url)
                                .map(|(id, _)| *id)
                                .collect();

                            for id in ids.into_iter() {
                                if let Some(relay_url) = tracker.resend_queue.remove(&id) {
                                    if let Some(event) = client.database().event_by_id(&id).await? {
                                        let event_id = relay.send_event(&event).await?;

                                        let output = Output {
                                            val: event_id,
                                            failed: HashMap::new(),
                                            success: HashSet::from([relay_url]),
                                        };

                                        tracker.sent_ids.insert(event_id);
                                        tracker.resent_ids.push(output);
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
        for (_, request) in self.auth_requests.read(cx).clone() {
            self.open_auth_request(request, window, cx);
        }
    }

    fn push_auth_request(&mut self, req: &AuthRequest, cx: &mut Context<Self>) {
        self.auth_requests.update(cx, |this, cx| {
            this.insert(req.url.clone(), req.to_owned());
            cx.notify();
        });
    }

    fn sending_auth_request(&mut self, challenge: &str, cx: &mut Context<Self>) {
        self.auth_requests.update(cx, |this, cx| {
            for (_, req) in this.iter_mut() {
                if req.challenge == challenge {
                    req.sending = true;
                    cx.notify();
                }
            }
        });
    }

    fn is_sending_auth_request(&self, challenge: &str, cx: &App) -> bool {
        if let Some(req) = self
            .auth_requests
            .read(cx)
            .iter()
            .find(|(_, req)| req.challenge == challenge)
        {
            req.1.sending
        } else {
            false
        }
    }

    fn remove_auth_request(&mut self, challenge: &str, cx: &mut Context<Self>) {
        self.auth_requests.update(cx, |this, cx| {
            this.retain(|_, r| r.challenge != challenge);
            cx.notify();
        });
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
        public_key: PublicKey,
        secret: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = Arc::new(account::init(public_key, secret, window, cx));
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

    fn set_required_dm_relays(&mut self, cx: &mut Context<Self>) {
        self.nip17_ready = false;
        cx.notify();
    }

    fn set_required_gossip_relays(&mut self, cx: &mut Context<Self>) {
        self.nip65_ready = false;
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

    fn on_reload_metadata(
        &mut self,
        _ev: &ReloadMetadata,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let states = app_state();
            let client = states.client();

            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
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
                .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
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
        reset(cx);
    }

    fn on_open_pubkey(&mut self, ev: &OpenPublicKey, window: &mut Window, cx: &mut Context<Self>) {
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

    fn on_copy_pubkey(&mut self, ev: &CopyPublicKey, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(bech32) = ev.0.to_bech32();
        cx.write_to_clipboard(ClipboardItem::new_string(bech32));
        window.push_notification(t!("common.copied"), cx);
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

    fn set_center_panel<P>(panel: P, window: &mut Window, cx: &mut App)
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

    fn render_keyring_installation(&mut self, window: &mut Window, cx: &mut App) {
        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .alert()
                .button_props(ModalButtonProps::default().ok_text(t!("common.continue")))
                .title(shared_t!("keyring_disable.label"))
                .child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .child(shared_t!("keyring_disable.body_1"))
                        .child(shared_t!("keyring_disable.body_2"))
                        .child(shared_t!("keyring_disable.body_3"))
                        .child(shared_t!("keyring_disable.body_4"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().danger_foreground)
                                .child(shared_t!("keyring_disable.body_5")),
                        ),
                )
        });
    }

    fn render_setup_gossip_relays_modal(&mut self, window: &mut Window, cx: &mut App) {
        let relays = default_nip65_relays();

        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .button_props(
                    ModalButtonProps::default()
                        .cancel_text(t!("common.configure"))
                        .ok_text(t!("common.use_default")),
                )
                .title(shared_t!("mailbox.modal"))
                .child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .child(shared_t!("mailbox.description"))
                        .child(
                            v_flex()
                                .gap_1()
                                .text_xs()
                                .text_color(cx.theme().text_muted)
                                .child(shared_t!("mailbox.write_label"))
                                .child(shared_t!("mailbox.read_label")),
                        )
                        .child(
                            div()
                                .font_semibold()
                                .text_xs()
                                .child(shared_t!("common.default")),
                        )
                        .child(v_flex().gap_1().children({
                            let mut items = Vec::with_capacity(relays.len());

                            for (url, metadata) in relays {
                                items.push(
                                    div()
                                        .h_7()
                                        .px_1p5()
                                        .h_flex()
                                        .justify_between()
                                        .rounded(cx.theme().radius)
                                        .bg(cx.theme().elevated_surface_background)
                                        .text_sm()
                                        .child(
                                            div()
                                                .line_height(relative(1.2))
                                                .child(SharedString::from(url.to_string())),
                                        )
                                        .when_some(metadata.as_ref(), |this, metadata| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .font_semibold()
                                                    .line_height(relative(1.2))
                                                    .child(SharedString::from(
                                                        metadata.to_string(),
                                                    )),
                                            )
                                        }),
                                );
                            }

                            items
                        })),
                )
                .on_cancel(|_, _window, _cx| {
                    // TODO: add configure relays
                    // true to close the modal
                    true
                })
                .on_ok(|_, window, cx| {
                    window
                        .spawn(cx, async move |cx| {
                            let states = app_state();
                            let relays = default_nip65_relays();
                            let result = states.set_nip65(relays).await;

                            cx.update(|window, cx| {
                                match result {
                                    Ok(_) => {
                                        window.close_modal(cx);
                                    }
                                    Err(e) => {
                                        window.push_notification(e.to_string(), cx);
                                    }
                                };
                            })
                            .ok();
                        })
                        .detach();

                    // false to keep modal open
                    false
                })
        })
    }

    fn render_setup_dm_relays_modal(window: &mut Window, cx: &mut App) {
        let relays = default_nip17_relays();

        window.open_modal(cx, move |this, _window, cx| {
            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .button_props(
                    ModalButtonProps::default()
                        .cancel_text(t!("common.configure"))
                        .ok_text(t!("common.use_default")),
                )
                .title(shared_t!("messaging.modal"))
                .child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .child(shared_t!("messaging.description"))
                        .child(
                            div()
                                .font_semibold()
                                .text_xs()
                                .child(shared_t!("common.default")),
                        )
                        .child(v_flex().gap_1().children({
                            let mut items = Vec::with_capacity(relays.len());

                            for url in relays {
                                items.push(
                                    div()
                                        .h_7()
                                        .px_1p5()
                                        .h_flex()
                                        .justify_between()
                                        .rounded(cx.theme().radius)
                                        .bg(cx.theme().elevated_surface_background)
                                        .text_sm()
                                        .child(
                                            div()
                                                .line_height(relative(1.2))
                                                .child(SharedString::from(url.to_string())),
                                        ),
                                );
                            }

                            items
                        })),
                )
                .on_cancel(|_, window, cx| {
                    let view = cx.new(|cx| SetupRelay::new(window, cx));
                    let weak_view = view.downgrade();

                    window.open_modal(cx, move |modal, _window, _cx| {
                        let weak_view = weak_view.clone();

                        modal
                            .confirm()
                            .title(shared_t!("relays.modal"))
                            .child(view.clone())
                            .button_props(ModalButtonProps::default().ok_text(t!("common.update")))
                            .on_ok(move |_, window, cx| {
                                weak_view
                                    .update(cx, |this, cx| {
                                        this.set_relays(window, cx);
                                    })
                                    .ok();
                                // true to close the modal
                                false
                            })
                    });

                    // true to close the modal
                    true
                })
                .on_ok(|_, window, cx| {
                    window
                        .spawn(cx, async move |cx| {
                            let states = app_state();
                            let relays = default_nip17_relays();
                            let result = states.set_nip17(relays).await;

                            cx.update(|window, cx| {
                                match result {
                                    Ok(_) => {
                                        window.close_modal(cx);
                                    }
                                    Err(e) => {
                                        window.push_notification(e.to_string(), cx);
                                    }
                                };
                            })
                            .ok();
                        })
                        .detach();

                    // false to keep modal open
                    false
                })
        })
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
                this.child(deferred(
                    h_flex()
                        .px_2()
                        .h_6()
                        .gap_1()
                        .text_xs()
                        .rounded_full()
                        .bg(cx.theme().surface_background)
                        .child(shared_t!("loading.label")),
                ))
            })
    }

    fn render_titlebar_right_side(
        &mut self,
        profile: &Profile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let updating = AutoUpdater::read_global(cx).status.is_updating();
        let updated = AutoUpdater::read_global(cx).status.is_updated();
        let auth_requests = self.auth_requests.read(cx).len();

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
            .when(auth_requests > 0, |this| {
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
            .when(!self.nip17_ready, |this| {
                this.child(
                    Button::new("setup-relays-button")
                        .icon(IconName::Info)
                        .label(t!("messaging.button"))
                        .warning()
                        .xsmall()
                        .rounded()
                        .on_click(move |_ev, window, cx| {
                            Self::render_setup_dm_relays_modal(window, cx);
                        }),
                )
            })
            .child(
                Button::new("user")
                    .small()
                    .reverse()
                    .transparent()
                    .icon(IconName::CaretDown)
                    .child(Avatar::new(profile.avatar(proxy)).size(rems(1.49)))
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
}

impl Render for ChatSpace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let registry = Registry::read_global(cx);

        // Only render titlebar child elements if user is logged in
        if let Some(public_key) = registry.signer_pubkey() {
            let profile = registry.get_person(&public_key, cx);

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
            .id(SharedString::from("chatspace"))
            .on_action(cx.listener(Self::on_settings))
            .on_action(cx.listener(Self::on_dark_mode))
            .on_action(cx.listener(Self::on_sign_out))
            .on_action(cx.listener(Self::on_open_pubkey))
            .on_action(cx.listener(Self::on_copy_pubkey))
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
