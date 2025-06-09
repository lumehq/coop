use std::collections::BTreeSet;
use std::ops::Range;
use std::time::Duration;

use async_utility::task::spawn;
use chats::room::{Room, RoomKind};
use chats::{ChatRegistry, RoomEmitter};
use common::debounced_delay::DebouncedDelay;
use common::profile::RenderProfile;
use element::DisplayRoom;
use global::constants::{DEFAULT_MODAL_WIDTH, SEARCH_RELAYS};
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, rems, uniform_list, AnyElement, App, AppContext, ClipboardItem, Context, Entity,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    RetainAllImageCache, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
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
            |this, _state, event, _window, cx| {
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
            },
        ));

        Self {
            name: "Chat Sidebar".into(),
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            find_debouncer: DebouncedDelay::new(),
            finding: false,
            trusted_only: false,
            indicator,
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
            let filter = Filter::new()
                .kind(Kind::Metadata)
                .search(query.to_lowercase())
                .limit(FIND_LIMIT);

            let events = shared_state()
                .client
                .fetch_events_from(SEARCH_RELAYS, filter, Duration::from_secs(3))
                .await?
                .into_iter()
                .unique_by(|event| event.pubkey)
                .collect_vec();

            let mut rooms = BTreeSet::new();
            let (tx, rx) = smol::channel::bounded::<Room>(10);

            spawn(async move {
                let signer = shared_state()
                    .client
                    .signer()
                    .await
                    .expect("signer is required");
                let public_key = signer.get_public_key().await.expect("error");

                for event in events.into_iter() {
                    let metadata = Metadata::from_json(event.content).unwrap_or_default();

                    let Some(target) = metadata.nip05.as_ref() else {
                        continue;
                    };

                    let Ok(verified) = nip05::verify(&event.pubkey, target, None).await else {
                        continue;
                    };

                    if !verified {
                        continue;
                    };

                    if let Ok(event) = EventBuilder::private_msg_rumor(event.pubkey, "")
                        .build(public_key)
                        .sign(&Keys::generate())
                        .await
                    {
                        if let Err(e) = tx.send(Room::new(&event).kind(RoomKind::Ongoing)).await {
                            log::error!("{e}")
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

        if query.starts_with("nevent1")
            || query.starts_with("naddr")
            || query.starts_with("nsec1")
            || query.starts_with("note1")
        {
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
                    .id("current-user")
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font_semibold()
                    .child(Avatar::new(profile.render_avatar()).size(rems(1.75)))
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
            .flex()
            .flex_col()
            .gap_3()
            // Account
            .when_some(
                shared_state().identity.read_blocking().as_ref(),
                |this, profile| this.child(self.render_account(profile, cx)),
            )
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
                this.child(div().px_2().w_full().flex().flex_col().gap_1().children({
                    let mut items = Vec::with_capacity(rooms.len());

                    for (ix, room) in rooms.into_iter().enumerate() {
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
                            move |this, range, _window, cx| {
                                this.render_uniform_item(&rooms, range, cx)
                            },
                        )
                        .h_full(),
                    ),
            )
    }
}
