use std::collections::BTreeSet;
use std::ops::Range;
use std::time::Duration;

use anyhow::{anyhow, Error};
use common::debounced_delay::DebouncedDelay;
use common::display::{ReadableTimestamp, TextUtils};
use global::constants::{BOOTSTRAP_RELAYS, SEARCH_RELAYS};
use global::{css, nostr_client, UnwrappingStatus};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, uniform_list, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Render, RetainAllImageCache,
    SharedString, Styled, Subscription, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use itertools::Itertools;
use list_item::RoomListItem;
use nostr_sdk::prelude::*;
use registry::room::{Room, RoomKind};
use registry::{Registry, RegistryEvent};
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState, TextInput};
use ui::popup_menu::{PopupMenu, PopupMenuExt};
use ui::{h_flex, v_flex, ContextModal, Icon, IconName, Selectable, Sizable, StyledExt};

use crate::actions::{GiftWrapManage, Reload};

mod list_item;

const FIND_DELAY: u64 = 600;
const FIND_LIMIT: usize = 10;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    Sidebar::new(window, cx)
}

pub struct Sidebar {
    name: SharedString,
    // Search
    find_input: Entity<InputState>,
    find_debouncer: DebouncedDelay<Self>,
    finding: bool,
    cancel_handle: Entity<Option<smol::channel::Sender<()>>>,
    local_result: Entity<Option<Vec<Entity<Room>>>>,
    global_result: Entity<Option<Vec<Entity<Room>>>>,
    // Rooms
    indicator: Entity<Option<RoomKind>>,
    active_filter: Entity<RoomKind>,
    // GPUI
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 2]>,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let active_filter = cx.new(|_| RoomKind::Ongoing);
        let indicator = cx.new(|_| None);
        let local_result = cx.new(|_| None);
        let global_result = cx.new(|_| None);
        let cancel_handle = cx.new(|_| None);

        let find_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("sidebar.find_or_start_conversation"))
        });

        let chats = Registry::global(cx);
        let mut subscriptions = smallvec![];

        subscriptions.push(cx.subscribe_in(
            &chats,
            window,
            move |this, _chats, event, _window, cx| {
                if let RegistryEvent::NewRequest(kind) = event {
                    this.indicator.update(cx, |this, cx| {
                        *this = Some(kind.to_owned());
                        cx.notify();
                    });
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            &find_input,
            window,
            |this, _state, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.search(window, cx),
                    InputEvent::Change(text) => {
                        // Clear the result when input is empty
                        if text.is_empty() {
                            this.clear_search_results(window, cx);
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
            },
        ));

        Self {
            name: "Sidebar".into(),
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            find_debouncer: DebouncedDelay::new(),
            finding: false,
            cancel_handle,
            indicator,
            active_filter,
            find_input,
            local_result,
            global_result,
            subscriptions,
        }
    }

    async fn request_metadata(client: &Client, public_key: PublicKey) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];
        let filter = Filter::new().author(public_key).kinds(kinds).limit(10);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        log::info!("Subscribe to get metadata for: {public_key}");

        Ok(())
    }

    async fn create_temp_room(identity: PublicKey, public_key: PublicKey) -> Result<Room, Error> {
        let client = nostr_client();
        let keys = Keys::generate();
        let builder = EventBuilder::private_msg_rumor(public_key, "");
        let event = builder.build(identity).sign(&keys).await?;

        // Request to get user's metadata
        Self::request_metadata(client, public_key).await?;

        // Create a temporary room
        let room = Room::new(&event).rearrange_by(identity);

        Ok(room)
    }

    async fn nip50(identity: PublicKey, query: &str) -> BTreeSet<Room> {
        let client = nostr_client();
        let timeout = Duration::from_secs(2);
        let mut rooms: BTreeSet<Room> = BTreeSet::new();

        let filter = Filter::new()
            .kind(Kind::Metadata)
            .search(query.to_lowercase())
            .limit(FIND_LIMIT);

        if let Ok(events) = client
            .fetch_events_from(SEARCH_RELAYS, filter, timeout)
            .await
        {
            // Process to verify the search results
            for event in events.into_iter().unique_by(|event| event.pubkey) {
                // Skip if author is match current user
                if event.pubkey == identity {
                    continue;
                }

                // Return a temporary room
                if let Ok(room) = Self::create_temp_room(identity, event.pubkey).await {
                    rooms.insert(room);
                }
            }
        }

        rooms
    }

    fn debounced_search(&self, window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            this.update_in(cx, |this, window, cx| {
                this.search(window, cx);
            })
            .ok();
        })
    }

    fn search_by_nip50(
        &mut self,
        query: &str,
        rx: smol::channel::Receiver<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let identity = Registry::read_global(cx).identity(cx).public_key();
        let query = query.to_owned();
        let query_cloned = query.clone();

        let task = smol::future::or(
            Tokio::spawn(cx, async move {
                let rooms = Self::nip50(identity, &query).await;
                Some(rooms)
            }),
            Tokio::spawn(cx, async move {
                let _ = rx.recv().await.is_ok();
                None
            }),
        );

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(Some(results)) => {
                    this.update_in(cx, |this, window, cx| {
                        let msg = t!("sidebar.empty", query = query_cloned);
                        let rooms = results.into_iter().map(|r| cx.new(|_| r)).collect_vec();

                        if rooms.is_empty() {
                            window.push_notification(msg, cx);
                        }

                        this.results(rooms, true, window, cx);
                    })
                    .ok();
                }
                // User cancelled the search
                Ok(None) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_finding(false, window, cx);
                            this.set_cancel_handle(None, cx);
                        })
                    })
                    .ok();
                }
                // Async task failed
                Err(e) => {
                    this.update_in(cx, |this, window, cx| {
                        window.push_notification(e.to_string(), cx);
                        this.set_finding(false, window, cx);
                        this.set_cancel_handle(None, cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn search_by_nip05(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let identity = Registry::read_global(cx).identity(cx).public_key();
        let address = query.to_owned();

        let task = Tokio::spawn(cx, async move {
            if let Ok(profile) = common::nip05::nip05_profile(&address).await {
                Self::create_temp_room(identity, profile.public_key).await
            } else {
                Err(anyhow!(t!("sidebar.addr_error")))
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(Ok(room)) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.results(vec![cx.new(|_| room)], true, window, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
                Ok(Err(e)) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            window.push_notification(e.to_string(), cx);
                            this.set_cancel_handle(None, cx);
                            this.set_finding(false, window, cx);
                        })
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            window.push_notification(e.to_string(), cx);
                            this.set_cancel_handle(None, cx);
                            this.set_finding(false, window, cx);
                        })
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn search_by_pubkey(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(public_key) = query.to_public_key() else {
            window.push_notification(t!("common.pubkey_invalid"), cx);
            self.set_finding(false, window, cx);
            return;
        };

        let identity = Registry::read_global(cx).identity(cx).public_key();
        let task: Task<Result<Room, Error>> = cx.background_spawn(async move {
            // Create a gift wrap event to represent as room
            Self::create_temp_room(identity, public_key).await
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(room) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            let registry = Registry::read_global(cx);
                            let result = registry.search_by_public_key(public_key, cx);

                            if !result.is_empty() {
                                this.results(result, false, window, cx);
                            } else {
                                this.results(vec![cx.new(|_| room)], true, window, cx);
                            }
                        })
                        .ok();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(e.to_string(), cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = smol::channel::bounded::<()>(1);
        let tx_clone = tx.clone();

        // Return if the query is empty
        if self.find_input.read(cx).value().is_empty() {
            return;
        }

        // Return if search is in progress
        if self.finding {
            if self.cancel_handle.read(cx).is_none() {
                window.push_notification(t!("sidebar.search_in_progress"), cx);
                return;
            } else {
                // This is a hack to cancel ongoing search request
                cx.background_spawn(async move {
                    tx.send(()).await.ok();
                })
                .detach();
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

        let chats = Registry::read_global(cx);
        // Get all local results with current query
        let local_results = chats.search(&query, cx);

        if !local_results.is_empty() {
            // Try to update with local results first
            self.results(local_results, false, window, cx);
        } else {
            // If no local results, try global search via NIP-50
            self.set_cancel_handle(Some(tx_clone), cx);
            self.search_by_nip50(&query, rx, window, cx);
        }
    }

    fn results(
        &mut self,
        rooms: Vec<Entity<Room>>,
        global: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.finding {
            self.set_finding(false, window, cx);
        }

        if self.cancel_handle.read(cx).is_some() {
            self.set_cancel_handle(None, cx);
        }

        if !rooms.is_empty() {
            if global {
                self.global_result.update(cx, |this, cx| {
                    *this = Some(rooms);
                    cx.notify();
                });
            } else {
                self.local_result.update(cx, |this, cx| {
                    *this = Some(rooms);
                    cx.notify();
                });
            }
        }
    }

    fn set_finding(&mut self, status: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.finding = status;
        // Disable the input to prevent duplicate requests
        self.find_input.update(cx, |this, cx| {
            this.set_disabled(status, cx);
            this.set_loading(status, cx);
        });

        cx.notify();
    }

    fn set_cancel_handle(
        &mut self,
        handle: Option<smol::channel::Sender<()>>,
        cx: &mut Context<Self>,
    ) {
        self.cancel_handle.update(cx, |this, cx| {
            *this = handle;
            cx.notify();
        });
    }

    fn clear_search_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Reset the input state
        if self.finding {
            self.set_finding(false, window, cx);
        }

        // Clear all local results
        self.local_result.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });

        // Clear all global results
        self.global_result.update(cx, |this, cx| {
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
        let room = if let Some(room) = Registry::read_global(cx).room(&id, cx) {
            room
        } else {
            let Some(result) = self.global_result.read(cx).as_ref() else {
                window.push_notification(t!("common.room_error"), cx);
                return;
            };

            let Some(room) = result.iter().find(|this| this.read(cx).id == id).cloned() else {
                window.push_notification(t!("common.room_error"), cx);
                return;
            };

            // Clear all search results
            self.clear_search_results(window, cx);

            room
        };

        Registry::global(cx).update(cx, |this, cx| {
            this.push_room(room, cx);
        });
    }

    fn on_reload(&mut self, _ev: &Reload, window: &mut Window, cx: &mut Context<Self>) {
        Registry::global(cx).update(cx, |this, cx| {
            this.load_rooms(window, cx);
        });
        window.push_notification(t!("common.refreshed"), cx);
    }

    fn on_manage(&mut self, _ev: &GiftWrapManage, window: &mut Window, cx: &mut Context<Self>) {
        let task: Task<Result<Vec<Relay>, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let css = css();
            let subscription = client.subscription(&css.gift_wrap_sub_id).await;
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
        window.open_modal(cx, move |this, _window, cx| {
            this.show_close(true)
                .overlay_closable(true)
                .keyboard(true)
                .title(shared_t!("manage_relays.modal"))
                .child(v_flex().pb_4().gap_2().children({
                    let mut items = Vec::with_capacity(relays.len());

                    for relay in relays.clone().into_iter() {
                        let url = relay.url().to_string();
                        let time = relay.stats().connected_at().to_human_time();

                        items.push(
                            h_flex()
                                .h_8()
                                .px_2()
                                .justify_between()
                                .text_xs()
                                .bg(cx.theme().elevated_surface_background)
                                .rounded(cx.theme().radius)
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .font_semibold()
                                        .child(
                                            Icon::new(IconName::Signal)
                                                .small()
                                                .text_color(cx.theme().danger_active)
                                                .when(relay.is_connected(), |this| {
                                                    this.text_color(gpui::green().alpha(0.75))
                                                }),
                                        )
                                        .child(url),
                                )
                                .child(
                                    div()
                                        .text_right()
                                        .text_color(cx.theme().text_muted)
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
            if let Some(room) = rooms.get(ix) {
                let this = room.read(cx);
                let room_id = this.id;
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
                        .created_at(this.created_at.to_ago())
                        .public_key(this.members[0])
                        .kind(this.kind)
                        .on_click(handler),
                )
            } else {
                items.push(RoomListItem::new(ix));
            }
        }

        items
    }
}

impl Panel for Sidebar {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
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
        let registry = Registry::read_global(cx);

        // Get rooms from either search results or the chat registry
        let rooms = if let Some(results) = self.local_result.read(cx).as_ref() {
            results.to_owned()
        } else if let Some(results) = self.global_result.read(cx).as_ref() {
            results.to_owned()
        } else {
            #[allow(clippy::collapsible_else_if)]
            if self.active_filter.read(cx) == &RoomKind::Ongoing {
                registry.ongoing_rooms(cx)
            } else {
                registry.request_rooms(cx)
            }
        };

        // Get total rooms count
        let mut total_rooms = rooms.len();

        // Add 3 dummy rooms to display as skeletons
        if registry.unwrapping_status.read(cx) != &UnwrappingStatus::Complete {
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
                div()
                    .relative()
                    .mt_3()
                    .px_2p5()
                    .w_full()
                    .h_7()
                    .flex_none()
                    .flex()
                    .child(
                        TextInput::new(&self.find_input)
                            .small()
                            .cleanable()
                            .appearance(true)
                            .suffix(
                                Button::new("find")
                                    .icon(IconName::Search)
                                    .tooltip(t!("sidebar.press_enter_to_search"))
                                    .transparent()
                                    .small(),
                            ),
                    ),
            )
            // Chat Rooms
            .child(
                v_flex()
                    .gap_1()
                    .flex_1()
                    .px_1p5()
                    .w_full()
                    .overflow_y_hidden()
                    .child(
                        div()
                            .px_1()
                            .h_flex()
                            .gap_2()
                            .flex_none()
                            .child(
                                Button::new("all")
                                    .label(t!("sidebar.all_button"))
                                    .tooltip(t!("sidebar.all_conversations_tooltip"))
                                    .when_some(self.indicator.read(cx).as_ref(), |this, kind| {
                                        this.when(kind == &RoomKind::Ongoing, |this| {
                                            this.child(
                                                div().size_1().rounded_full().bg(cx.theme().cursor),
                                            )
                                        })
                                    })
                                    .small()
                                    .cta()
                                    .bold()
                                    .secondary()
                                    .rounded(ButtonRounded::Full)
                                    .selected(self.filter(&RoomKind::Ongoing, cx))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_filter(RoomKind::Ongoing, cx);
                                    })),
                            )
                            .child(
                                Button::new("requests")
                                    .label(t!("sidebar.requests_button"))
                                    .tooltip(t!("sidebar.requests_tooltip"))
                                    .when_some(self.indicator.read(cx).as_ref(), |this, kind| {
                                        this.when(kind != &RoomKind::Ongoing, |this| {
                                            this.child(
                                                div().size_1().rounded_full().bg(cx.theme().cursor),
                                            )
                                        })
                                    })
                                    .small()
                                    .cta()
                                    .bold()
                                    .secondary()
                                    .rounded(ButtonRounded::Full)
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
                                    .text_xs()
                                    .child(
                                        Button::new("option")
                                            .icon(IconName::Ellipsis)
                                            .xsmall()
                                            .ghost()
                                            .rounded(ButtonRounded::Full)
                                            .popup_menu(move |this, _window, _cx| {
                                                this.menu("Reload", Box::new(Reload))
                                                    .menu("Relay Status", Box::new(GiftWrapManage))
                                            }),
                                    ),
                            ),
                    )
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
