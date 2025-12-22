use std::ops::Range;
use std::time::Duration;

use account::Account;
use anyhow::{anyhow, Error};
use chat::{ChatEvent, ChatRegistry, Room, RoomKind};
use common::{DebouncedDelay, RenderedTimestamp, TextUtils, BOOTSTRAP_RELAYS, SEARCH_RELAYS};
use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, relative, uniform_list, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    RetainAllImageCache, SharedString, Styled, Subscription, Task, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::menu::DropdownMenu;
use gpui_component::{
    h_flex, v_flex, ActiveTheme, Icon, IconName, Selectable, Sizable, StyledExt, WindowExt,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use list_item::RoomListItem;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use state::{NostrRegistry, GIFTWRAP_SUBSCRIPTION};

use crate::actions::{RelayStatus, Reload};

mod list_item;

const FIND_DELAY: u64 = 600;
const FIND_LIMIT: usize = 20;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    cx.new(|cx| Sidebar::new(window, cx))
}

/// Sidebar
pub struct Sidebar {
    focus_handle: FocusHandle,

    /// Image cache
    image_cache: Entity<RetainAllImageCache>,

    /// Search results
    search_results: Entity<Option<Vec<Entity<Room>>>>,

    /// Async search operation
    search_task: Option<Task<()>>,

    /// Input for searching
    find_input: Entity<InputState>,

    /// Input debouncer
    find_debouncer: DebouncedDelay<Self>,

    /// Whether searching is in progress
    finding: bool,

    /// New message indicator
    indicator: Entity<Option<RoomKind>>,

    /// Current chat rooms filter
    active_filter: Entity<RoomKind>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 3]>,
}

impl Sidebar {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat = ChatRegistry::global(cx);
        let active_filter = cx.new(|_| RoomKind::Ongoing);
        let indicator = cx.new(|_| None);
        let search_results = cx.new(|_| None);

        let find_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("sidebar.search_label")));

        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Clear the image cache when sidebar is closed
            cx.on_release_in(window, move |this, window, cx| {
                this.image_cache.update(cx, |this, cx| {
                    this.clear(window, cx);
                });
            }),
        );

        subscriptions.push(
            // Subscribe for registry new events
            cx.subscribe_in(&chat, window, move |this, _, event, _window, cx| {
                if let ChatEvent::NewChatRequest(kind) = event {
                    this.indicator.update(cx, |this, cx| {
                        *this = Some(kind.to_owned());
                        cx.notify();
                    });
                }
            }),
        );

        subscriptions.push(
            // Subscribe for find input events
            cx.subscribe_in(&find_input, window, |this, state, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.search(window, cx);
                    }
                    InputEvent::Change => {
                        // Clear the result when input is empty
                        if state.read(cx).value().is_empty() {
                            this.clear(window, cx);
                        } else {
                            // Run debounced search
                            this.find_debouncer.fire_new(
                                Duration::from_millis(FIND_DELAY),
                                window,
                                cx,
                                |this, window, cx| this.debounced_search(window, cx),
                            );
                        }
                    }
                    _ => {}
                }
            }),
        );

        Self {
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            find_debouncer: DebouncedDelay::new(),
            finding: false,
            indicator,
            active_filter,
            find_input,
            search_results,
            search_task: None,
            _subscriptions: subscriptions,
        }
    }

    async fn nip50(client: &Client, query: &str) -> Result<Vec<Event>, Error> {
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        let filter = Filter::new()
            .kind(Kind::Metadata)
            .search(query.to_lowercase())
            .limit(FIND_LIMIT);

        let mut stream = client
            .stream_events_from(SEARCH_RELAYS, filter, Duration::from_secs(3))
            .await?;

        let mut results: Vec<Event> = Vec::with_capacity(FIND_LIMIT);

        while let Some(event) = stream.next().await {
            // Skip if author is match current user
            if event.pubkey == public_key {
                continue;
            }

            // Skip if the event has already been added
            if results.iter().any(|this| this.pubkey == event.pubkey) {
                continue;
            }

            results.push(event);
        }

        if results.is_empty() {
            return Err(anyhow!("No results for query {query}"));
        }

        // Get all public keys
        let public_keys: Vec<PublicKey> = results.iter().map(|event| event.pubkey).collect();

        // Fetch metadata and contact lists if public keys is not empty
        if !public_keys.is_empty() {
            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
            let filter = Filter::new()
                .kinds(vec![Kind::Metadata, Kind::ContactList])
                .limit(public_keys.len() * 2)
                .authors(public_keys);

            client
                .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                .await?;
        }

        Ok(results)
    }

    fn debounced_search(&self, window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            this.update_in(cx, |this, window, cx| {
                this.search(window, cx);
            })
            .ok();
        })
    }

    fn search_by_nip50(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let account = Account::global(cx);
        let public_key = account.read(cx).public_key();

        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let query = query.to_owned();

        self.search_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = Self::nip50(&client, &query).await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(results) => {
                        let rooms = results
                            .into_iter()
                            .map(|event| {
                                cx.new(|_| Room::new(None, public_key, vec![event.pubkey]))
                            })
                            .collect();

                        this.set_results(rooms, cx);
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
                this.set_finding(false, window, cx);
            })
            .ok();
        }));
    }

    fn search_by_nip05(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let address = query.to_owned();

        let task = Tokio::spawn(cx, async move {
            match common::nip05_profile(&address).await {
                Ok(profile) => {
                    let signer = client.signer().await?;
                    let public_key = signer.get_public_key().await?;
                    let receivers = vec![profile.public_key];
                    let room = Room::new(None, public_key, receivers);

                    Ok(room)
                }
                Err(e) => Err(anyhow!(e)),
            }
        });

        self.search_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Ok(room)) => {
                        this.set_results(vec![cx.new(|_| room)], cx);
                    }
                    Ok(Err(e)) => {
                        window.push_notification(e.to_string(), cx);
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                }
                this.set_finding(false, window, cx);
            })
            .ok();
        }));
    }

    fn search_by_pubkey(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let Ok(public_key) = query.to_public_key() else {
            window.push_notification("Public Key is invalid", cx);
            self.set_finding(false, window, cx);
            return;
        };

        let task: Task<Result<Room, Error>> = cx.background_spawn(async move {
            let signer = client.signer().await?;
            let author = signer.get_public_key().await?;

            let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
            let receivers = vec![public_key];
            let room = Room::new(None, author, receivers);

            let filter = Filter::new()
                .kinds(vec![Kind::Metadata, Kind::ContactList])
                .author(public_key)
                .limit(2);

            client
                .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
                .await?;

            Ok(room)
        });

        self.search_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(room) => {
                        let chat = ChatRegistry::global(cx);
                        let local_results = chat.read(cx).search_by_public_key(public_key, cx);

                        if !local_results.is_empty() {
                            this.set_results(local_results, cx);
                        } else {
                            this.set_results(vec![cx.new(|_| room)], cx);
                        }
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                };
                this.set_finding(false, window, cx);
            })
            .ok();
        }));
    }

    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Return if the query is empty
        if self.find_input.read(cx).value().is_empty() {
            return;
        }

        // Return if search is in progress
        if self.finding {
            if self.search_task.is_none() {
                window.push_notification("There is another search in progress", cx);
                return;
            } else {
                // Cancel ongoing search request
                self.search_task = None;
            }
        }

        let input = self.find_input.read(cx).value();
        let query = input.to_string();

        // Block the input until the search process completes
        self.set_finding(true, window, cx);

        // Process to search by pubkey if query starts with npub or nprofile
        if query.starts_with("npub1") || query.starts_with("nprofile1") {
            self.search_by_pubkey(&query, window, cx);
            return;
        };

        // Process to search by NIP05 if query is a valid NIP-05 identifier (name@domain.tld)
        if query.split('@').count() == 2 {
            let parts: Vec<&str> = query.split('@').collect();
            if !parts[0].is_empty() && !parts[1].is_empty() && parts[1].contains('.') {
                self.search_by_nip05(&query, window, cx);
                return;
            }
        }

        // Get all local results with current query
        let chat = ChatRegistry::global(cx);
        let local_results = chat.read(cx).search(&query, cx);

        // Try to update with local results first
        if !local_results.is_empty() {
            self.set_results(local_results, cx);
            return;
        };

        // If no local results, try global search via NIP-50
        self.search_by_nip50(&query, window, cx);
    }

    fn set_results(&mut self, rooms: Vec<Entity<Room>>, cx: &mut Context<Self>) {
        self.search_results.update(cx, |this, cx| {
            *this = Some(rooms);
            cx.notify();
        });
    }

    fn set_finding(&mut self, status: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.finding = status;
        // Disable the input to prevent duplicate requests
        self.find_input.update(cx, |this, cx| {
            this.set_loading(status, window, cx);
        });

        cx.notify();
    }

    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Reset the input state
        if self.finding {
            self.set_finding(false, window, cx);
        }

        // Clear all local results
        self.search_results.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });
    }

    fn filter(&self, kind: &RoomKind, cx: &Context<Self>) -> bool {
        self.active_filter.read(cx) == kind
    }

    fn set_filter(&mut self, kind: RoomKind, cx: &mut Context<Self>) {
        self.indicator.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });
        self.active_filter.update(cx, |this, cx| {
            *this = kind;
            cx.notify();
        });
    }

    fn open_room(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let chat = ChatRegistry::global(cx);
        let room = if let Some(room) = chat.read(cx).room(&id, cx) {
            room
        } else {
            let Some(result) = self.search_results.read(cx).as_ref() else {
                window.push_notification(shared_t!("common.room_error"), cx);
                return;
            };

            let Some(room) = result.iter().find(|this| this.read(cx).id == id).cloned() else {
                window.push_notification(shared_t!("common.room_error"), cx);
                return;
            };

            // Clear all search results
            self.clear(window, cx);

            room
        };

        chat.update(cx, |this, cx| {
            this.push_room(room, cx);
        });
    }

    fn on_reload(&mut self, _ev: &Reload, window: &mut Window, cx: &mut Context<Self>) {
        ChatRegistry::global(cx).update(cx, |this, cx| {
            this.get_rooms(cx);
        });
        window.push_notification(shared_t!("common.refreshed"), cx);
    }

    fn on_manage(&mut self, _ev: &RelayStatus, window: &mut Window, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        let task: Task<Result<Vec<Relay>, Error>> = cx.background_spawn(async move {
            let id = SubscriptionId::new(GIFTWRAP_SUBSCRIPTION);
            let subscription = client.subscription(&id).await;

            let mut relays: Vec<Relay> = vec![];

            for (url, _filter) in subscription.into_iter() {
                relays.push(client.pool().relay(url).await?);
            }

            Ok(relays)
        });

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(relays) = task.await {
                this.update_in(cx, |this, window, cx| {
                    this.manage_relays(relays, window, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn manage_relays(&mut self, relays: Vec<Relay>, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, move |this, _window, cx| {
            this.close_button(true)
                .overlay_closable(true)
                .keyboard(true)
                .title(shared_t!("manage_relays.modal"))
                .child(v_flex().pb_4().gap_2().children({
                    let mut items = Vec::with_capacity(relays.len());

                    for relay in relays.clone().into_iter() {
                        let url = relay.url().to_string();
                        let time = relay.stats().connected_at().to_ago();
                        let connected = relay.is_connected();

                        items.push(
                            h_flex()
                                .h_8()
                                .px_2()
                                .justify_between()
                                .text_xs()
                                .bg(cx.theme().list)
                                .rounded(cx.theme().radius)
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .font_semibold()
                                        .child(
                                            Icon::new(IconName::Plus)
                                                .small()
                                                .text_color(cx.theme().danger_active)
                                                .when(connected, |this| {
                                                    this.text_color(gpui::green().alpha(0.75))
                                                }),
                                        )
                                        .child(url),
                                )
                                .child(
                                    div()
                                        .text_right()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(shared_t!("manage_relays.time", t = time)),
                                ),
                        );
                    }

                    items
                }))
        });
    }

    fn list_items(
        &self,
        rooms: &[Entity<Room>],
        range: Range<usize>,
        cx: &Context<Self>,
    ) -> Vec<impl IntoElement> {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let mut items = Vec::with_capacity(range.end - range.start);

        for ix in range {
            let Some(room) = rooms.get(ix) else {
                items.push(RoomListItem::new(ix));
                continue;
            };

            let this = room.read(cx);
            let room_id = this.id;
            let member = this.display_member(cx);

            let handler = cx.listener({
                move |this, _, window, cx| {
                    this.open_room(room_id, window, cx);
                }
            });

            items.push(
                RoomListItem::new(ix)
                    .room_id(room_id)
                    .name(this.display_name(cx))
                    .avatar(this.display_image(proxy, cx))
                    .public_key(member.public_key())
                    .kind(this.kind)
                    .created_at(this.created_at.to_ago())
                    .on_click(handler),
            )
        }

        items
    }
}

impl Panel for Sidebar {
    fn panel_name(&self) -> &'static str {
        "Sidebar"
    }
}

impl EventEmitter<PanelEvent> for Sidebar {}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chat = ChatRegistry::global(cx);
        let loading = chat.read(cx).loading;

        // Get rooms from either search results or the chat registry
        let rooms = if let Some(results) = self.search_results.read(cx).as_ref() {
            results.to_owned()
        } else {
            // Filter rooms based on the active filter
            if self.active_filter.read(cx) == &RoomKind::Ongoing {
                chat.read(cx).ongoing_rooms(cx)
            } else {
                chat.read(cx).request_rooms(cx)
            }
        };

        // Get total rooms count
        let mut total_rooms = rooms.len();

        // Add 3 dummy rooms to display as skeletons
        if loading {
            total_rooms += 3
        }

        v_flex()
            .on_action(cx.listener(Self::on_reload))
            .on_action(cx.listener(Self::on_manage))
            .image_cache(self.image_cache.clone())
            .size_full()
            .relative()
            .gap_3()
            // Search Input
            .child(
                div().flex_none().mt_2().px_2().child(
                    Input::new(&self.find_input)
                        .cleanable(true)
                        .when(!self.finding, |this| {
                            this.suffix(
                                Button::new("find")
                                    .icon(IconName::Search)
                                    .tooltip("Press Enter to search")
                                    .small()
                                    .text(),
                            )
                        }),
                ),
            )
            // Chat Rooms
            .child(
                v_flex()
                    .flex_1()
                    .px_2()
                    .gap_2()
                    .overflow_y_hidden()
                    .child(
                        div()
                            .flex_none()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("all")
                                    .label("All")
                                    .tooltip("All ongoing conversations")
                                    .when_some(self.indicator.read(cx).as_ref(), |this, kind| {
                                        this.when(kind == &RoomKind::Ongoing, |this| {
                                            this.child(
                                                div().size_1().rounded_full().bg(cx.theme().caret),
                                            )
                                        })
                                    })
                                    .small()
                                    .ghost()
                                    .rounded(cx.theme().radius)
                                    .selected(self.filter(&RoomKind::Ongoing, cx))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_filter(RoomKind::Ongoing, cx);
                                    })),
                            )
                            .child(
                                Button::new("requests")
                                    .label("Requests")
                                    .tooltip("Incoming new conversations")
                                    .when_some(self.indicator.read(cx).as_ref(), |this, kind| {
                                        this.when(kind != &RoomKind::Ongoing, |this| {
                                            this.child(
                                                div().size_1().rounded_full().bg(cx.theme().caret),
                                            )
                                        })
                                    })
                                    .small()
                                    .ghost()
                                    .rounded(cx.theme().radius)
                                    .selected(!self.filter(&RoomKind::Ongoing, cx))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_filter(RoomKind::default(), cx);
                                    })),
                            )
                            .child(
                                h_flex()
                                    .flex_1()
                                    .w_full()
                                    .justify_end()
                                    .items_center()
                                    .child(
                                        Button::new("option")
                                            .icon(IconName::Ellipsis)
                                            .small()
                                            .ghost()
                                            .rounded(cx.theme().radius)
                                            .dropdown_menu(move |this, _window, _cx| {
                                                this.menu("Reload", Box::new(Reload))
                                                    .menu("Relay Status", Box::new(RelayStatus))
                                            }),
                                    ),
                            ),
                    )
                    .when(!loading && total_rooms == 0, |this| {
                        this.map(|this| {
                            if self.filter(&RoomKind::Ongoing, cx) {
                                this.child(deferred(
                                    v_flex()
                                        .py_2()
                                        .px_1p5()
                                        .gap_1p5()
                                        .items_center()
                                        .justify_center()
                                        .text_center()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .line_height(relative(1.25))
                                                .child(shared_t!("sidebar.no_conversations")),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .line_height(relative(1.25))
                                                .child(shared_t!("sidebar.no_conversations_label")),
                                        ),
                                ))
                            } else {
                                this.child(deferred(
                                    v_flex()
                                        .py_2()
                                        .px_1p5()
                                        .gap_1p5()
                                        .items_center()
                                        .justify_center()
                                        .text_center()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .line_height(relative(1.25))
                                                .child(shared_t!("sidebar.no_requests")),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .line_height(relative(1.25))
                                                .child(shared_t!("sidebar.no_requests_label")),
                                        ),
                                ))
                            }
                        })
                    })
                    .child(
                        uniform_list(
                            "rooms",
                            total_rooms,
                            cx.processor(move |this, range, _window, cx| {
                                this.list_items(&rooms, range, cx)
                            }),
                        )
                        .h_full(),
                    ),
            )
    }
}
