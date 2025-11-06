use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Error};
use gpui::{
    App, AppContext, Context, Entity, Global, IntoElement, ParentElement, SharedString, Styled,
    Subscription, Task, Window,
};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::notification::Notification;
use ui::{v_flex, ContextModal, Disableable, IconName, Sizable};

const AUTH_MESSAGE: &str =
    "Approve the authentication request to allow Coop to continue sending or receiving events.";

pub fn init(window: &mut Window, cx: &mut App) {
    RelayAuth::set_global(cx.new(|cx| RelayAuth::new(window, cx)), cx);
}

struct GlobalRelayAuth(Entity<RelayAuth>);

impl Global for GlobalRelayAuth {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthRequest {
    pub url: RelayUrl,
    pub challenge: String,
    pub sending: bool,
}

impl AuthRequest {
    pub fn new(challenge: impl Into<String>, url: RelayUrl) -> Self {
        Self {
            challenge: challenge.into(),
            sending: false,
            url,
        }
    }
}

#[derive(Debug)]
pub struct RelayAuth {
    /// Entity for managing auth requests
    requests: Entity<HashMap<RelayUrl, AuthRequest>>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl RelayAuth {
    /// Retrieve the global relay auth state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalRelayAuth>().0.clone()
    }

    /// Set the global relay auth instance
    fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalRelayAuth(state));
    }

    /// Create a new relay auth instance
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let requests: Entity<HashMap<RelayUrl, AuthRequest>> = cx.new(|_| HashMap::new());

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        subscriptions.push(
            // Observe the current state
            cx.observe_in(&requests, window, |this, requests, window, cx| {
                let auto_auth = AppSettings::get_auto_auth(cx);
                let requests = requests.read(cx).clone();

                for (url, request) in requests.into_iter() {
                    let is_authenticated = AppSettings::read_global(cx).is_authenticated(&url);

                    if auto_auth && is_authenticated {
                        // Automatically authenticate if the relay is authenticated before
                        this.response(request, window, cx);
                    } else {
                        // Otherwise open the auth request popup
                        this.ask_for_approval(request, window, cx);
                    }
                }
            }),
        );

        tasks.push(
            // Handle notifications
            cx.spawn({
                let client = Arc::clone(&client);
                async move |this, cx| {
                    let mut notifications = client.notifications();
                    let mut challenges: HashSet<Cow<'_, str>> = HashSet::new();

                    while let Ok(notification) = notifications.recv().await {
                        let RelayPoolNotification::Message { message, relay_url } = notification
                        else {
                            // Skip if the notification is not a message
                            continue;
                        };

                        if let RelayMessage::Auth { challenge } = message {
                            if challenges.insert(challenge.clone()) {
                                this.update(cx, |this, cx| {
                                    let request = AuthRequest::new(challenge, relay_url.clone());
                                    this.insert(relay_url, request, cx);
                                })
                                .expect("Entity has been released")
                            }
                        }
                    }
                }
            }),
        );

        Self {
            requests,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    /// Respond to an authentication request.
    fn response(&mut self, req: AuthRequest, window: &mut Window, cx: &mut Context<Self>) {
        let settings = AppSettings::global(cx);
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let tracker = nostr.read(cx).tracker();

        let challenge = req.challenge.to_owned();
        let url = req.url.to_owned();

        let challenge_clone = challenge.clone();
        let url_clone = url.clone();

        // Set Coop is sending auth for this request
        self.set_sending(&challenge, cx);

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
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
                            let mut tracker = tracker.write().await;

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

        self._tasks.push(cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(_) => {
                    this.update_in(cx, |this, window, cx| {
                        this.remove(&challenge, cx);

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
        }));
    }

    /// Inserts a new authentication request into the entity.
    fn insert(&mut self, relay_url: RelayUrl, request: AuthRequest, cx: &mut App) {
        self.requests.update(cx, |this, cx| {
            this.insert(relay_url, request);
            cx.notify();
        });
    }

    /// Sets the sending status of an authentication request.
    fn set_sending(&mut self, challenge: &str, cx: &mut Context<Self>) {
        self.requests.update(cx, |this, cx| {
            for (_, req) in this.iter_mut() {
                if req.challenge == challenge {
                    req.sending = true;
                    cx.notify();
                }
            }
        });
    }

    /// Checks if an authentication request is currently being sent.
    fn is_sending(&self, challenge: &str, cx: &App) -> bool {
        self.requests
            .read(cx)
            .values()
            .find(|req| req.challenge == challenge)
            .is_some_and(|req| req.sending)
    }

    /// Removes an authentication request from the list.
    fn remove(&mut self, challenge: &str, cx: &mut Context<Self>) {
        self.requests.update(cx, |this, cx| {
            this.retain(|_, r| r.challenge != challenge);
            cx.notify();
        });
    }

    /// Push a popup to approve the authentication request.
    fn ask_for_approval(&mut self, req: AuthRequest, window: &mut Window, cx: &mut Context<Self>) {
        let url = SharedString::from(req.url.clone().to_string());

        // Get a weak reference to the current entity
        let entity = cx.entity().downgrade();

        let note = Notification::new()
            .custom_id(SharedString::from(&req.challenge))
            .autohide(false)
            .icon(IconName::Info)
            .title(SharedString::from("Authentication Required"))
            .content(move |_window, cx| {
                v_flex()
                    .gap_2()
                    .text_sm()
                    .child(SharedString::from(AUTH_MESSAGE))
                    .child(
                        v_flex()
                            .py_1()
                            .px_1p5()
                            .rounded_sm()
                            .text_xs()
                            .bg(cx.theme().warning_background)
                            .text_color(cx.theme().warning_foreground)
                            .child(url.clone()),
                    )
                    .into_any_element()
            })
            .action(move |_window, cx| {
                let entity = entity.clone();
                let req = req.clone();

                // Get the loading state
                let loading = entity
                    .read_with(cx, |this, cx| this.is_sending(&req.challenge, cx))
                    .unwrap_or_default();

                Button::new("approve")
                    .label("Approve")
                    .small()
                    .primary()
                    .loading(loading)
                    .disabled(loading)
                    .on_click(move |_ev, window, cx| {
                        _ = entity.update(cx, |this, cx| {
                            this.response(req.clone(), window, cx);
                        });
                    })
            });

        // Push the notification to the current window
        window.push_notification(note, cx);

        // Focus the window if it's not active
        if !window.is_window_active() {
            window.activate_window();
        }
    }

    /// Get the number of pending requests.
    pub fn pending_requests(&self, cx: &App) -> usize {
        self.requests.read(cx).iter().count()
    }

    /// Reask for approval for all pending requests.
    pub fn re_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (_url, request) in self.requests.read(cx).clone() {
            self.ask_for_approval(request, window, cx);
        }
    }
}
