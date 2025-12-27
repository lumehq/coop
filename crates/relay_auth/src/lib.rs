use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use anyhow::{anyhow, Error};
use gpui::{
    App, AppContext, Context, Entity, Global, IntoElement, ParentElement, SharedString, Styled,
    Subscription, Task, Window,
};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use state::{client, event_store};
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
}

impl Hash for AuthRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.challenge.hash(state);
    }
}

impl AuthRequest {
    pub fn new(challenge: impl Into<String>, url: RelayUrl) -> Self {
        Self {
            challenge: challenge.into(),
            url,
        }
    }
}

#[derive(Debug)]
pub struct RelayAuth {
    /// Entity for managing auth requests
    requests: HashSet<AuthRequest>,

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
        let entity = cx.entity();

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        // Channel for communication between Nostr and GPUI
        let (tx, rx) = flume::bounded::<AuthRequest>(100);

        subscriptions.push(
            // Observe the current state
            cx.observe_in(&entity, window, |this, _, window, cx| {
                let settings = AppSettings::global(cx);
                let auto_auth = AppSettings::get_auto_auth(cx);

                for req in this.requests.clone().into_iter() {
                    let is_authenticated = settings.read(cx).is_authenticated(&req.url);

                    if auto_auth && is_authenticated {
                        // Automatically authenticate if the relay is authenticated before
                        this.response(req.to_owned(), window, cx);
                    } else {
                        // Otherwise open the auth request popup
                        this.ask_for_approval(req.to_owned(), window, cx);
                    }
                }
            }),
        );

        tasks.push(
            // Handle nostr notifications
            cx.background_spawn(async move {
                let client = client();
                let mut notifications = client.notifications();
                let mut challenges: HashSet<Cow<'_, str>> = HashSet::new();

                while let Ok(notification) = notifications.recv().await {
                    let RelayPoolNotification::Message { message, relay_url } = notification else {
                        // Skip if the notification is not a message
                        continue;
                    };

                    if let RelayMessage::Auth { challenge } = message {
                        if challenges.insert(challenge.clone()) {
                            let auth = AuthRequest::new(challenge, relay_url);
                            tx.send_async(auth).await.ok();
                        };
                    }
                }
            }),
        );

        tasks.push(
            // Update GPUI state
            cx.spawn(async move |this, cx| {
                while let Ok(request) = rx.recv_async().await {
                    this.update(cx, |this, cx| {
                        this.requests.insert(request);
                        cx.notify();
                    })
                    .ok();
                }
            }),
        );

        Self {
            requests: HashSet::new(),
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    /// Get the number of pending requests.
    pub fn pending_requests(&self, _cx: &App) -> usize {
        self.requests.len()
    }

    /// Reask for approval for all pending requests.
    pub fn re_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for request in self.requests.clone().into_iter() {
            self.ask_for_approval(request, window, cx);
        }
    }

    /// Respond to an authentication request.
    fn response(&mut self, req: AuthRequest, window: &mut Window, cx: &mut Context<Self>) {
        let settings = AppSettings::global(cx);

        let challenge = req.challenge.to_owned();
        let url = req.url.to_owned();

        let challenge_clone = challenge.clone();
        let url_clone = url.clone();

        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = client();
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
                            let mut event_store = event_store().write().await;

                            let ids: Vec<EventId> = event_store
                                .resend_queue
                                .iter()
                                .filter(|(_, url)| relay_url == *url)
                                .map(|(id, _)| *id)
                                .collect();

                            for id in ids.into_iter() {
                                if let Some(relay_url) = event_store.resend_queue.remove(&id) {
                                    if let Some(event) = client.database().event_by_id(&id).await? {
                                        let event_id = relay.send_event(&event).await?;

                                        let output = Output {
                                            val: event_id,
                                            failed: HashMap::new(),
                                            success: HashSet::from([relay_url]),
                                        };

                                        event_store.sent_ids.insert(event_id);
                                        event_store.resent_ids.push(output);
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

        self._tasks.push(
            // Handle response in the background
            cx.spawn_in(window, async move |this, cx| {
                match task.await {
                    Ok(_) => {
                        this.update_in(cx, |this, window, cx| {
                            // Clear the current notification
                            window.clear_notification_by_id(SharedString::from(&challenge), cx);

                            // Push a new notification
                            window.push_notification(format!("{url} has been authenticated"), cx);

                            // Save the authenticated relay to automatically authenticate future requests
                            settings.update(cx, |this, cx| {
                                this.push_relay(&url, cx);
                            });

                            // Remove the challenge from the list of pending authentications
                            this.requests.remove(&req);
                            cx.notify();
                        })
                        .expect("Entity has been released");
                    }
                    Err(e) => {
                        this.update_in(cx, |_, window, cx| {
                            window.push_notification(Notification::error(e.to_string()), cx);
                        })
                        .expect("Entity has been released");
                    }
                };
            }),
        );
    }

    /// Push a popup to approve the authentication request.
    fn ask_for_approval(&mut self, req: AuthRequest, window: &mut Window, cx: &mut Context<Self>) {
        let url = SharedString::from(req.url.clone().to_string());
        let entity = cx.entity().downgrade();
        let loading = Rc::new(Cell::new(false));

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
            .action(move |_window, _cx| {
                let entity = entity.clone();
                let req = req.clone();

                Button::new("approve")
                    .label("Approve")
                    .small()
                    .primary()
                    .loading(loading.get())
                    .disabled(loading.get())
                    .on_click({
                        let loading = Rc::clone(&loading);
                        move |_ev, window, cx| {
                            // Set loading state to true
                            loading.set(true);
                            // Process to approve the request
                            entity
                                .update(cx, |this, cx| {
                                    this.response(req.clone(), window, cx);
                                })
                                .expect("Entity has been released");
                        }
                    })
            });

        // Push the notification to the current window
        window.push_notification(note, cx);

        // Focus the window if it's not active
        if !window.is_window_hovered() {
            window.activate_window();
        }
    }
}
