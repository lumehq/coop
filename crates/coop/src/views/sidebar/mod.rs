use std::collections::BTreeSet;
use std::ops::Range;
use std::time::Duration;

use anyhow::Error;
use chats::room::{Room, RoomKind};
use chats::{ChatRegistry, RoomEmitter};
use common::debounced_delay::DebouncedDelay;
use common::profile::RenderProfile;
use element::DisplayRoom;
use global::constants::{DEFAULT_MODAL_WIDTH, SEARCH_RELAYS};
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, AnyElement, App, AppContext, ClipboardItem, Context,
    Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, RetainAllImageCache, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Task, Window,
};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
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
        let trusted_only = AppSettings::get_global(cx).settings.only_show_trusted;

        let find_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find or start a conversation"));

        let chats = ChatRegistry::global(cx);
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
                            this.clear_search_results(cx);
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
            name: "Chat Sidebar".into(),
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            find_debouncer: DebouncedDelay::new(),
            finding: false,
            trusted_only,
            indicator,
            active_filter,
            find_input,
            local_result,
            global_result,
            subscriptions,
        }
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

    fn search_by_nip50(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.to_owned();
        let task: Task<Result<BTreeSet<Room>, Error>> = cx.background_spawn(async move {
            let client = shared_state().client();

            let filter = Filter::new()
                .kind(Kind::Metadata)
                .search(query.to_lowercase())
                .limit(FIND_LIMIT);

            let events = client
                .fetch_events_from(SEARCH_RELAYS, filter, Duration::from_secs(5))
                .await?
                .into_iter()
                .unique_by(|event| event.pubkey)
                .collect_vec();

            let mut rooms = BTreeSet::new();

            // Process to verify the search results
            if !events.is_empty() {
                let (tx, rx) = smol::channel::bounded::<Room>(events.len());

                nostr_sdk::async_utility::task::spawn(async move {
                    let signer = client.signer().await.unwrap();
                    let public_key = signer.get_public_key().await.unwrap();

                    for event in events.into_iter() {
                        let metadata = Metadata::from_json(event.content).unwrap_or_default();

                        log::info!("metadata: {:?}", metadata);

                        let Some(target) = metadata.nip05.as_ref() else {
                            // Skip if NIP-05 is not found
                            continue;
                        };

                        let Ok(verified) = nip05::verify(&event.pubkey, target, None).await else {
                            // Skip if NIP-05 verification fails
                            continue;
                        };

                        if !verified {
                            // Skip if NIP-05 is not valid
                            continue;
                        };

                        if let Ok(event) = EventBuilder::private_msg_rumor(event.pubkey, "")
                            .build(public_key)
                            .sign(&Keys::generate())
                            .await
                        {
                            if let Err(e) = tx.send(Room::new(&event).kind(RoomKind::Ongoing)).await
                            {
                                log::error!("Send error: {e}")
                            }
                        }
                    }
                });

                while let Ok(room) = rx.recv().await {
                    rooms.insert(room);
                }
            }

            Ok(rooms)
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(result) => {
                    this.update(cx, |this, cx| {
                        let result = result
                            .into_iter()
                            .map(|room| cx.new(|_| room))
                            .collect_vec();

                        this.global_result(result, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(e.to_string()).title("Search Error"),
                            cx,
                        );
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn search_by_user(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = if query.starts_with("npub1") {
            PublicKey::parse(query).ok()
        } else if query.starts_with("nprofile1") {
            Nip19Profile::from_bech32(query)
                .map(|nip19| nip19.public_key)
                .ok()
        } else {
            None
        };

        let Some(public_key) = public_key else {
            window.push_notification("Public Key is not valid", cx);
            self.set_finding(false, cx);
            return;
        };

        let task: Task<Result<(Profile, Room), Error>> = cx.background_spawn(async move {
            let client = shared_state().client();
            let signer = client.signer().await.unwrap();
            let user_pubkey = signer.get_public_key().await.unwrap();

            let metadata = client
                .fetch_metadata(public_key, Duration::from_secs(3))
                .await?
                .unwrap_or_default();

            let event = EventBuilder::private_msg_rumor(public_key, "")
                .build(user_pubkey)
                .sign(&Keys::generate())
                .await?;

            let profile = Profile::new(public_key, metadata);
            let room = Room::new(&event);

            Ok((profile, room))
        });

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok((profile, room)) => {
                    this.update(cx, |this, cx| {
                        let chats = ChatRegistry::global(cx);
                        let result = chats
                            .read(cx)
                            .search_by_public_key(profile.public_key(), cx);

                        this.local_result(result, cx);
                        this.global_result(vec![cx.new(|_| room)], cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(e.to_string()).title("Search Error"),
                            cx,
                        );
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.find_input.read(cx).value().to_string();

        // Return if search is in progress
        if self.finding {
            window.push_notification("There is another search in progress", cx);
            return;
        }

        // Return if the query is empty
        if query.is_empty() {
            window.push_notification("Cannot search with an empty query", cx);
            return;
        }

        // Return if the query starts with "nsec1" or "note1"
        if query.starts_with("nsec1") || query.starts_with("note1") {
            window.push_notification("Coop does not support searching with this query", cx);
            return;
        }

        // Block the input until the search process completes
        self.set_finding(true, cx);

        // Process to search by user if query starts with npub or nprofile
        if query.starts_with("npub1") || query.starts_with("nprofile1") {
            self.search_by_user(&query, window, cx);
            return;
        };

        let chats = ChatRegistry::global(cx);
        let result = chats.read(cx).search(&query, cx);

        if result.is_empty() {
            // There are no current rooms matching this query, so proceed with global search via NIP-50
            self.search_by_nip50(&query, window, cx);
        } else {
            self.local_result(result, cx);
        }
    }

    fn global_result(&mut self, rooms: Vec<Entity<Room>>, cx: &mut Context<Self>) {
        if self.finding {
            self.set_finding(false, cx);
        }

        self.global_result.update(cx, |this, cx| {
            *this = Some(rooms);
            cx.notify();
        });
    }

    fn local_result(&mut self, rooms: Vec<Entity<Room>>, cx: &mut Context<Self>) {
        if self.finding {
            self.set_finding(false, cx);
        }

        self.local_result.update(cx, |this, cx| {
            *this = Some(rooms);
            cx.notify();
        });
    }

    fn set_finding(&mut self, status: bool, cx: &mut Context<Self>) {
        self.finding = status;
        cx.notify();
        // Disable the input to prevent duplicate requests
        self.find_input.update(cx, |this, cx| {
            this.set_disabled(status, cx);
            this.set_loading(status, cx);
        });
    }

    fn clear_search_results(&mut self, cx: &mut Context<Self>) {
        // Reset the input state
        if self.finding {
            self.set_finding(false, cx);
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
        let room = if let Some(room) = ChatRegistry::get_global(cx).room(&id, cx) {
            room
        } else {
            let Some(result) = self.global_result.read(cx).as_ref() else {
                window.push_notification("Failed to open room. Please try again later.", cx);
                return;
            };

            let Some(room) = result.iter().find(|this| this.read(cx).id == id).cloned() else {
                window.push_notification("Failed to open room. Please try again later.", cx);
                return;
            };

            // Clear all search results
            self.clear_search_results(cx);

            room
        };

        ChatRegistry::global(cx).update(cx, |this, cx| {
            this.push_room(room, cx);
        });
    }

    fn open_compose(&self, window: &mut Window, cx: &mut Context<Self>) {
        let compose = compose::init(window, cx);

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title("Direct Messages")
                .width(px(DEFAULT_MODAL_WIDTH))
                .child(compose.clone())
        });
    }

    fn open_loading_modal(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_modal(cx, move |this, _window, cx| {
            const BODY_1: &str =
                "Coop is downloading all your messages from the messaging relays. \
                Depending on your total number of messages, this process may take up to \
                15 minutes if you're using Nostr Connect.";
            const BODY_2: &str =
                "Please be patient - you only need to do this full download once. \
                Next time, Coop will only download new messages.";
            const DESCRIPTION: &str = "You still can use the app normally \
                while messages are processing in the background";

            this.child(
                div()
                    .pt_8()
                    .pb_4()
                    .px_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .text_sm()
                            .child(BODY_1)
                            .child(BODY_2),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child(DESCRIPTION),
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
                    .child(Avatar::new(profile.render_avatar(proxy)).size(rems(1.75)))
                    .child(profile.render_name())
                    .on_click(cx.listener({
                        let Ok(public_key) = profile.public_key().to_bech32();
                        let item = ClipboardItem::new_string(public_key);

                        move |_, _, window, cx| {
                            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                            cx.write_to_primary(item.clone());
                            #[cfg(any(target_os = "windows", target_os = "macos"))]
                            cx.write_to_clipboard(item.clone());

                            window.push_notification("User's NPUB is copied", cx);
                        }
                    })),
            )
            .child(
                Button::new("compose")
                    .icon(IconName::PlusFill)
                    .tooltip("Create DM or Group DM")
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
        let mut items = Vec::with_capacity(range.end - range.start);

        for ix in range {
            if let Some(room) = rooms.get(ix) {
                let this = room.read(cx);
                let id = this.id;
                let ago = this.ago();
                let label = this.display_name(cx);
                let img = this.display_image(cx);

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
        let chats = ChatRegistry::get_global(cx);

        // Get rooms from either search results or the chat registry
        let rooms = if let Some(results) = self.local_result.read(cx) {
            results.to_owned()
        } else {
            #[allow(clippy::collapsible_else_if)]
            if self.active_filter.read(cx) == &RoomKind::Ongoing {
                chats.ongoing_rooms(cx)
            } else {
                chats.request_rooms(self.trusted_only, cx)
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
            .when_some(Identity::get_global(cx).profile(), |this, profile| {
                this.child(self.account(&profile, cx))
            })
            // Search Input
            .child(
                div().px_3().w_full().h_7().flex_none().child(
                    TextInput::new(&self.find_input).small().suffix(
                        Button::new("find")
                            .icon(IconName::Search)
                            .tooltip("Press Enter to search")
                            .transparent()
                            .small(),
                    ),
                ),
            )
            // Global Search Results
            .when_some(self.global_result.read(cx).as_ref(), |this, rooms| {
                this.child(div().px_2().w_full().flex().flex_col().gap_1().children({
                    let mut items = Vec::with_capacity(rooms.len());

                    for (ix, room) in rooms.iter().enumerate() {
                        let this = room.read(cx);
                        let id = this.id;
                        let label = this.display_name(cx);
                        let img = this.display_image(cx);

                        let handler = cx.listener(move |this, _, window, cx| {
                            this.open_room(id, window, cx);
                        });

                        items.push(DisplayRoom::new(ix).img(img).label(label).on_click(handler))
                    }

                    items
                }))
            })
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
                                            .label("All")
                                            .tooltip("All ongoing conversations")
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
                                            .label("Requests")
                                            .tooltip("Incoming new conversations")
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
                                        .tooltip("Only show rooms from trusted contacts")
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
                    .when(chats.loading, |this| {
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
            .when(chats.loading, |this| {
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
                                            .child("Retrieving messages..."),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_muted)
                                            .child("This may take some time"),
                                    ),
                            )
                            // Info button
                            .child(
                                Button::new("help")
                                    .icon(IconName::Info)
                                    .tooltip("Why you're seeing this")
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
