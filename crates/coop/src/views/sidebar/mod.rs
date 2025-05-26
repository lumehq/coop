use std::{collections::BTreeSet, ops::Range, time::Duration};

use account::Account;
use async_utility::task::spawn;
use chats::{
    room::{Room, RoomKind},
    ChatRegistry,
};

use common::{debounced_delay::DebouncedDelay, profile::SharedProfile};
use element::DisplayRoom;
use global::{constants::SEARCH_RELAYS, get_client};
use gpui::{
    div, img, prelude::FluentBuilder, uniform_list, AnyElement, App, AppContext, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, ObjectFit, ParentElement, Render,
    RetainAllImageCache, SharedString, Styled, StyledImage, Subscription, Task, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::{
        dock::DockPlacement,
        panel::{Panel, PanelEvent},
    },
    input::{InputEvent, InputState, TextInput},
    popup_menu::{PopupMenu, PopupMenuExt},
    skeleton::Skeleton,
    IconName, Selectable, Sizable, StyledExt,
};

use crate::chatspace::{AddPanel, ModalKind, PanelKind, ToggleModal};

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
    active_filter: Entity<RoomKind>,
    trusted_only: bool,
    // GPUI
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let active_filter = cx.new(|_| RoomKind::Ongoing);
        let local_result = cx.new(|_| None);
        let global_result = cx.new(|_| None);

        let find_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find or start a conversation"));

        let mut subscriptions = smallvec![];

        subscriptions.push(
            cx.subscribe_in(&find_input, window, |this, _, event, _, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.search(cx),
                    InputEvent::Change(text) => {
                        // Clear the result when input is empty
                        if text.is_empty() {
                            this.clear_search_results(cx);
                        } else {
                            // Run debounced search
                            this.find_debouncer.fire_new(
                                Duration::from_millis(FIND_DELAY),
                                cx,
                                |this, cx| this.debounced_search(cx),
                            );
                        }
                    }
                    _ => {}
                }
            }),
        );

        Self {
            name: "Chat Sidebar".into(),
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            find_debouncer: DebouncedDelay::new(),
            finding: false,
            trusted_only: false,
            active_filter,
            find_input,
            local_result,
            global_result,
            subscriptions,
        }
    }

    fn debounced_search(&self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            this.update(cx, |this, cx| {
                this.search(cx);
            })
            .ok();
        })
    }

    fn nip50_search(&self, cx: &App) -> Task<Result<BTreeSet<Room>, Error>> {
        let query = self.find_input.read(cx).value().clone();

        cx.background_spawn(async move {
            let client = get_client();

            let filter = Filter::new()
                .kind(Kind::Metadata)
                .search(query.to_lowercase())
                .limit(FIND_LIMIT);

            let events = client
                .fetch_events_from(SEARCH_RELAYS, filter, Duration::from_secs(3))
                .await?
                .into_iter()
                .unique_by(|event| event.pubkey)
                .collect_vec();

            let mut rooms = BTreeSet::new();
            let (tx, rx) = smol::channel::bounded::<Room>(10);

            spawn(async move {
                let client = get_client();
                let signer = client.signer().await.unwrap();

                for event in events.into_iter() {
                    let metadata = Metadata::from_json(event.content).unwrap_or_default();

                    if let Some(target) = metadata.nip05.as_ref() {
                        if let Ok(verify) = nip05::verify(&event.pubkey, target, None).await {
                            if verify {
                                if let Ok(event) = EventBuilder::private_msg_rumor(event.pubkey, "")
                                    .sign(&signer)
                                    .await
                                {
                                    let room = Room::new(&event);
                                    _ = tx.send(room).await;
                                }
                            }
                        }
                    }
                }
            });

            while let Ok(room) = rx.recv().await {
                rooms.insert(room);
            }

            Ok(rooms)
        })
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.find_input.read(cx).value();
        let result = ChatRegistry::get_global(cx).search(query.as_ref(), cx);

        // Return if query is empty
        if query.is_empty() {
            return;
        }

        // Return if search is in progress
        if self.finding {
            return;
        }

        // Block the UI until the search process completes
        self.set_finding(true, cx);

        // Disable the search input to prevent duplicate requests
        self.find_input.update(cx, |this, cx| {
            this.set_disabled(true, cx);
            this.set_loading(true, cx);
        });

        if !result.is_empty() {
            self.set_finding(false, cx);

            self.find_input.update(cx, |this, cx| {
                this.set_disabled(false, cx);
                this.set_loading(false, cx);
            });

            self.local_result.update(cx, |this, cx| {
                *this = Some(result);
                cx.notify();
            });
        } else {
            let task = self.nip50_search(cx);

            cx.spawn(async move |this, cx| {
                if let Ok(result) = task.await {
                    this.update(cx, |this, cx| {
                        let result = result
                            .into_iter()
                            .map(|room| cx.new(|_| room))
                            .collect_vec();

                        this.set_finding(false, cx);

                        this.find_input.update(cx, |this, cx| {
                            this.set_disabled(false, cx);
                            this.set_loading(false, cx);
                        });

                        this.global_result.update(cx, |this, cx| {
                            *this = Some(result);
                            cx.notify();
                        });
                    })
                    .ok();
                }
            })
            .detach();
        }
    }

    fn set_finding(&mut self, status: bool, cx: &mut Context<Self>) {
        self.finding = status;
        cx.notify();
    }

    fn clear_search_results(&mut self, cx: &mut Context<Self>) {
        self.local_result.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });
        self.global_result.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });
    }

    fn open_new_room(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(result) = self.global_result.read(cx).as_ref() {
            if let Some(room) = result.iter().find(|this| this.read(cx).id == id).cloned() {
                ChatRegistry::global(cx).update(cx, |this, cx| {
                    this.push_room(room, cx);
                });
                window.dispatch_action(
                    Box::new(AddPanel::new(PanelKind::Room(id), DockPlacement::Center)),
                    cx,
                );
                self.clear_search_results(cx);
            }
        }
    }

    fn filter(&self, kind: &RoomKind, cx: &Context<Self>) -> bool {
        self.active_filter.read(cx) == kind
    }

    fn set_filter(&mut self, kind: RoomKind, cx: &mut Context<Self>) {
        self.active_filter.update(cx, |this, cx| {
            *this = kind;
            cx.notify();
        })
    }

    fn set_trusted_only(&mut self, cx: &mut Context<Self>) {
        self.trusted_only = !self.trusted_only;
        cx.notify();
    }

    fn render_account(&self, profile: &Profile, cx: &Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .h_8()
            .flex_none()
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font_semibold()
                    .child(
                        div()
                            .flex_shrink_0()
                            .size_7()
                            .rounded_full()
                            .overflow_hidden()
                            .child(
                                img(profile.shared_avatar())
                                    .size_full()
                                    .rounded_full()
                                    .object_fit(ObjectFit::Fill),
                            ),
                    )
                    .child(profile.shared_name()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("user")
                            .icon(IconName::Ellipsis)
                            .small()
                            .ghost()
                            .rounded(ButtonRounded::Full)
                            .popup_menu(|this, _window, _cx| {
                                this.menu(
                                    "Profile",
                                    Box::new(ToggleModal {
                                        modal: ModalKind::Profile,
                                    }),
                                )
                                .menu(
                                    "Relays",
                                    Box::new(ToggleModal {
                                        modal: ModalKind::Relay,
                                    }),
                                )
                            }),
                    )
                    .child(
                        Button::new("compose")
                            .icon(IconName::PlusFill)
                            .tooltip("Create DM or Group DM")
                            .small()
                            .primary()
                            .rounded(ButtonRounded::Full)
                            .on_click(cx.listener(|_, _, window, cx| {
                                window.dispatch_action(
                                    Box::new(ToggleModal {
                                        modal: ModalKind::Compose,
                                    }),
                                    cx,
                                );
                            })),
                    ),
            )
    }

    fn render_skeleton(&self, total: i32) -> impl IntoIterator<Item = impl IntoElement> {
        (0..total).map(|_| {
            div()
                .h_9()
                .w_full()
                .px_1p5()
                .flex()
                .items_center()
                .gap_2()
                .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                .child(Skeleton::new().w_40().h_4().rounded_sm())
        })
    }

    fn render_uniform_item(
        &self,
        rooms: &[Entity<Room>],
        range: Range<usize>,
        is_search: bool,
        cx: &Context<Self>,
    ) -> Vec<impl IntoElement> {
        let mut items = Vec::with_capacity(range.end - range.start);

        for ix in range {
            if let Some(room) = rooms.get(ix) {
                let room = room.read(cx);

                let id = room.id;
                let ago = room.ago();
                let label = room.display_name(cx);
                let img = room.display_image(cx).map(img);

                let handler = cx.listener(move |this, _event, window, cx| {
                    if is_search {
                        this.open_new_room(id, window, cx);
                    } else {
                        window.dispatch_action(
                            Box::new(AddPanel::new(PanelKind::Room(id), DockPlacement::Center)),
                            cx,
                        );
                    }
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

        let (rooms, is_search) = if let Some(results) = self.local_result.read(cx) {
            (results.to_owned(), true)
        } else {
            #[allow(clippy::collapsible_else_if)]
            if self.active_filter.read(cx) == &RoomKind::Ongoing {
                (chats.ongoing_rooms(cx), false)
            } else {
                (chats.request_rooms(self.trusted_only, cx), false)
            }
        };

        div()
            .image_cache(self.image_cache.clone())
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            // Account
            .when_some(Account::get_global(cx).profile_ref(), |this, profile| {
                this.child(self.render_account(profile, cx))
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
            .when_some(self.global_result.read(cx).clone(), |this, rooms| {
                this.child(
                    div().px_1().w_full().flex_1().overflow_y_hidden().child(
                        uniform_list(
                            cx.entity(),
                            "results",
                            rooms.len(),
                            move |this, range, _, cx| {
                                this.render_uniform_item(&rooms, range, true, cx)
                            },
                        )
                        .h_full(),
                    ),
                )
            })
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
                    .when(chats.wait_for_eose, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .children(self.render_skeleton(10)),
                        )
                    })
                    .child(
                        uniform_list(
                            cx.entity(),
                            "rooms",
                            rooms.len(),
                            move |this, range, _, cx| {
                                this.render_uniform_item(&rooms, range, is_search, cx)
                            },
                        )
                        .h_full(),
                    ),
            )
    }
}
