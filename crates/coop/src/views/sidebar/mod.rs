use std::collections::BTreeSet;
use std::ops::Range;
use std::time::Duration;

use anyhow::{anyhow, Error};
use common::debounced_delay::DebouncedDelay;
use common::display::DisplayProfile;
use common::nip05::nip05_verify;
use element::DisplayRoom;
use global::constants::{BOOTSTRAP_RELAYS, DEFAULT_MODAL_WIDTH, SEARCH_RELAYS};
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, AnyElement, App, AppContext, ClipboardItem, Context,
    Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, RetainAllImageCache, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Task, Window,
};
use gpui_tokio::Tokio;
use i18n::t;
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::room::{Room, RoomKind};
use registry::{Registry, RoomEmitter};
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::input::{InputEvent, InputState, TextInput};
use ui::popup_menu::PopupMenu;
use ui::skeleton::Skeleton;
use ui::{ContextModal, IconName, Selectable, Sizable, StyledExt};

use crate::views::compose;

mod element;

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
    trusted_only: bool,
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
                if let RoomEmitter::Request(kind) = event {
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
            trusted_only: false,
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

        Ok(())
    }

    async fn create_temp_room(identity: PublicKey, public_key: PublicKey) -> Result<Room, Error> {
        let keys = Keys::generate();
        let builder = EventBuilder::private_msg_rumor(public_key, "");
        let event = builder.build(identity).sign(&keys).await?;
        let room = Room::new(&event).kind(RoomKind::Ongoing);

        Ok(room)
    }

    async fn nip50(identity: PublicKey, query: &str) -> BTreeSet<Room> {
        let client = nostr_client();
        let timeout = Duration::from_secs(2);
        let mut rooms: BTreeSet<Room> = BTreeSet::new();
        let mut processed: BTreeSet<PublicKey> = BTreeSet::new();

        let filter = Filter::new()
            .kind(Kind::Metadata)
            .search(query.to_lowercase())
            .limit(FIND_LIMIT);

        if let Ok(events) = client
            .fetch_events_from(SEARCH_RELAYS, filter, timeout)
            .await
        {
            // Process to verify the search results
            for event in events.into_iter() {
                if processed.contains(&event.pubkey) {
                    continue;
                }
                processed.insert(event.pubkey);

                let metadata = Metadata::from_json(event.content).unwrap_or_default();

                // Skip if NIP-05 is not found
                let Some(target) = metadata.nip05.as_ref() else {
                    continue;
                };

                // Skip if NIP-05 is not valid or failed to verify
                if !nip05_verify(event.pubkey, target).await.unwrap_or(false) {
                    continue;
                };

                if let Ok(room) = Self::create_temp_room(identity, event.pubkey).await {
                    rooms.insert(room);
                }
            }
        }

        rooms
    }

    fn debounced_search(&self, window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.search(window, cx);
                })
                .ok();
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
        let Some(identity) = Identity::read_global(cx).public_key() else {
            // User is not logged in. Stop searching
            self.set_finding(false, window, cx);
            self.set_cancel_handle(None, cx);
            return;
        };

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
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            let msg = t!("sidebar.empty", query = query_cloned);
                            let rooms = results.into_iter().map(|r| cx.new(|_| r)).collect_vec();

                            if rooms.is_empty() {
                                window.push_notification(msg, cx);
                            }

                            this.results(rooms, true, window, cx);
                        })
                        .ok();
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
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            window.push_notification(e.to_string(), cx);
                            this.set_finding(false, window, cx);
                            this.set_cancel_handle(None, cx);
                        })
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn search_by_nip05(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(identity) = Identity::read_global(cx).public_key() else {
            // User is not logged in. Stop searching
            self.set_finding(false, window, cx);
            self.set_cancel_handle(None, cx);
            return;
        };

        let address = query.to_owned();

        let task = Tokio::spawn(cx, async move {
            let client = nostr_client();

            if let Ok(profile) = common::nip05::nip05_profile(&address).await {
                let public_key = profile.public_key;
                // Request for user metadata
                Self::request_metadata(client, public_key).await.ok();
                // Return a temporary room
                Self::create_temp_room(identity, public_key).await
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
        let Ok(public_key) = common::parse_pubkey_from_str(query) else {
            window.push_notification(t!("common.pubkey_invalid"), cx);
            self.set_finding(false, window, cx);
            return;
        };

        let Some(identity) = Identity::read_global(cx).public_key() else {
            // User is not logged in. Stop searching
            self.set_finding(false, window, cx);
            return;
        };

        let task: Task<Result<Room, Error>> = cx.background_spawn(async move {
            let client = nostr_client();

            // Request metadata for this user
            Self::request_metadata(client, public_key).await?;

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

    fn set_trusted_only(&mut self, cx: &mut Context<Self>) {
        self.trusted_only = !self.trusted_only;
        cx.notify();
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

    fn open_compose(&self, window: &mut Window, cx: &mut Context<Self>) {
        let compose = compose::init(window, cx);
        let title = SharedString::new(t!("sidebar.direct_messages"));

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title(title.clone())
                .width(px(DEFAULT_MODAL_WIDTH))
                .child(compose.clone())
        });
    }

    fn open_loading_modal(&self, window: &mut Window, cx: &mut Context<Self>) {
        let title = SharedString::new(t!("sidebar.loading_modal_title"));
        let text_1 = SharedString::new(t!("sidebar.loading_modal_body_1"));
        let text_2 = SharedString::new(t!("sidebar.loading_modal_body_2"));
        let desc = SharedString::new(t!("sidebar.loading_modal_description"));

        window.open_modal(cx, move |this, _window, cx| {
            this.title(title.clone()).child(
                div()
                    .px_4()
                    .pb_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .text_sm()
                            .child(text_1.clone())
                            .child(text_2.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child(desc.clone()),
                    ),
            )
        });
    }

    fn account(&self, profile: &Profile, cx: &Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;

        div()
            .px_3()
            .h_8()
            .flex_none()
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .id("current-user")
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font_semibold()
                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(1.75)))
                    .child(profile.display_name())
                    .on_click(cx.listener({
                        let Ok(public_key) = profile.public_key().to_bech32();
                        let item = ClipboardItem::new_string(public_key);

                        move |_, _, window, cx| {
                            cx.write_to_clipboard(item.clone());
                            window.push_notification(t!("common.copied"), cx);
                        }
                    })),
            )
            .child(
                Button::new("compose")
                    .icon(IconName::PlusFill)
                    .tooltip(t!("sidebar.dm_tooltip"))
                    .small()
                    .primary()
                    .rounded(ButtonRounded::Full)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_compose(window, cx);
                    })),
            )
    }

    fn skeletons(&self, total: i32) -> impl IntoIterator<Item = impl IntoElement> {
        (0..total).map(|_| {
            div()
                .h_9()
                .w_full()
                .px_1p5()
                .flex()
                .items_center()
                .gap_2()
                .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_between()
                        .child(Skeleton::new().w_32().h_2p5().rounded_sm())
                        .child(Skeleton::new().w_6().h_2p5().rounded_sm()),
                )
        })
    }

    fn list_items(
        &self,
        rooms: &[Entity<Room>],
        range: Range<usize>,
        cx: &Context<Self>,
    ) -> Vec<impl IntoElement> {
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;
        let mut items = Vec::with_capacity(range.end - range.start);

        for ix in range {
            if let Some(room) = rooms.get(ix) {
                let this = room.read(cx);
                let id = this.id;
                let ago = this.ago();
                let label = this.display_name(cx);
                let img = this.display_image(proxy, cx);

                let handler = cx.listener(move |this, _, window, cx| {
                    this.open_room(id, window, cx);
                });

                items.push(
                    DisplayRoom::new(ix)
                        .img(img)
                        .label(label)
                        .description(ago)
                        .on_click(handler),
                )
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
        let profile = Identity::read_global(cx)
            .public_key()
            .map(|pk| registry.get_person(&pk, cx));

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
                registry.request_rooms(self.trusted_only, cx)
            }
        };

        div()
            .image_cache(self.image_cache.clone())
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .gap_3()
            // Account
            .when_some(profile, |this, profile| {
                this.child(self.account(&profile, cx))
            })
            // Search Input
            .child(
                div()
                    .relative()
                    .px_3()
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
                div()
                    .px_2()
                    .w_full()
                    .flex_1()
                    .overflow_y_hidden()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex_none()
                            .px_1()
                            .w_full()
                            .h_9()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new("all")
                                            .label(t!("sidebar.all_button"))
                                            .tooltip(t!("sidebar.all_conversations_tooltip"))
                                            .when_some(
                                                self.indicator.read(cx).as_ref(),
                                                |this, kind| {
                                                    this.when(kind == &RoomKind::Ongoing, |this| {
                                                        this.child(
                                                            div()
                                                                .size_1()
                                                                .rounded_full()
                                                                .bg(cx.theme().cursor),
                                                        )
                                                    })
                                                },
                                            )
                                            .small()
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
                                            .when_some(
                                                self.indicator.read(cx).as_ref(),
                                                |this, kind| {
                                                    this.when(kind != &RoomKind::Ongoing, |this| {
                                                        this.child(
                                                            div()
                                                                .size_1()
                                                                .rounded_full()
                                                                .bg(cx.theme().cursor),
                                                        )
                                                    })
                                                },
                                            )
                                            .small()
                                            .bold()
                                            .secondary()
                                            .rounded(ButtonRounded::Full)
                                            .selected(!self.filter(&RoomKind::Ongoing, cx))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_filter(RoomKind::Unknown, cx);
                                            })),
                                    ),
                            )
                            .when(!self.filter(&RoomKind::Ongoing, cx), |this| {
                                this.child(
                                    Button::new("trusted")
                                        .tooltip(t!("sidebar.trusted_contacts_tooltip"))
                                        .map(|this| {
                                            if self.trusted_only {
                                                this.icon(IconName::FilterFill)
                                            } else {
                                                this.icon(IconName::Filter)
                                            }
                                        })
                                        .small()
                                        .transparent()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.set_trusted_only(cx);
                                        })),
                                )
                            }),
                    )
                    .when(registry.loading, |this| {
                        this.child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .children(self.skeletons(1)),
                        )
                    })
                    .child(
                        uniform_list(
                            "rooms",
                            rooms.len(),
                            cx.processor(move |this, range, _window, cx| {
                                this.list_items(&rooms, range, cx)
                            }),
                        )
                        .h_full(),
                    ),
            )
            .when(registry.loading, |this| {
                this.child(
                    div().absolute().bottom_4().px_4().child(
                        div()
                            .p_1()
                            .w_full()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(cx.theme().panel_background)
                            .shadow_sm()
                            // Empty div
                            .child(div().size_6().flex_shrink_0())
                            // Loading indicator
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .text_center()
                                    .child(
                                        div()
                                            .font_semibold()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .line_height(relative(1.2))
                                            .child(Indicator::new().xsmall())
                                            .child(SharedString::new(t!(
                                                "sidebar.retrieving_messages"
                                            ))),
                                    )
                                    .child(div().text_color(cx.theme().text_muted).child(
                                        SharedString::new(t!(
                                            "sidebar.retrieving_messages_description"
                                        )),
                                    )),
                            )
                            // Info button
                            .child(
                                Button::new("help")
                                    .icon(IconName::Info)
                                    .tooltip(t!("sidebar.why_seeing_this_tooltip"))
                                    .small()
                                    .ghost()
                                    .rounded(ButtonRounded::Full)
                                    .flex_shrink_0()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_loading_modal(window, cx)
                                    })),
                            ),
                    ),
                )
            })
    }
}
