use std::{
    cmp::Reverse,
    collections::{BTreeSet, HashSet},
    time::Duration,
};

use account::Account;
use async_utility::task::spawn;
use chats::{
    room::{Room, RoomKind},
    ChatRegistry,
};

use common::{debounced_delay::DebouncedDelay, profile::SharedProfile};
use folder::{Folder, FolderItem, Parent};
use global::{constants::SEARCH_RELAYS, get_client};
use gpui::{
    div, img, prelude::FluentBuilder, AnyElement, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::{
    button::{Button, ButtonCustomVariant, ButtonRounded, ButtonVariants},
    dock_area::{
        dock::DockPlacement,
        panel::{Panel, PanelEvent},
    },
    input::{InputEvent, TextInput},
    popup_menu::{PopupMenu, PopupMenuExt},
    skeleton::Skeleton,
    IconName, Sizable, StyledExt,
};

use crate::chatspace::{AddPanel, ModalKind, PanelKind, ToggleModal};

mod folder;

const FIND_DELAY: u64 = 600;
const FIND_LIMIT: usize = 10;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    Sidebar::new(window, cx)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    Ongoing,
    Incoming,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubItem {
    Trusted,
    Unknown,
}

pub struct Sidebar {
    name: SharedString,
    // Search
    find_input: Entity<TextInput>,
    find_debouncer: DebouncedDelay<Self>,
    finding: bool,
    local_result: Entity<Option<Vec<Entity<Room>>>>,
    global_result: Entity<Option<Vec<Entity<Room>>>>,
    // Layout
    split_into_folders: bool,
    active_items: HashSet<Item>,
    active_subitems: HashSet<SubItem>,
    // GPUI
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let scroll_handle = ScrollHandle::default();

        let mut active_items = HashSet::with_capacity(2);
        active_items.insert(Item::Ongoing);

        let mut active_subitems = HashSet::with_capacity(2);
        active_subitems.insert(SubItem::Trusted);
        active_subitems.insert(SubItem::Unknown);

        let local_result = cx.new(|_| None);
        let global_result = cx.new(|_| None);
        let find_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .small()
                .text_size(ui::Size::XSmall)
                .suffix(|window, cx| {
                    Button::new("find")
                        .icon(IconName::Search)
                        .tooltip("Press Enter to search")
                        .small()
                        .custom(
                            ButtonCustomVariant::new(window, cx)
                                .active(gpui::transparent_black())
                                .color(gpui::transparent_black())
                                .hover(gpui::transparent_black())
                                .foreground(cx.theme().text_placeholder),
                        )
                })
                .placeholder("Find or start a conversation")
        });

        let mut subscriptions = smallvec![];

        subscriptions.push(
            cx.subscribe_in(&find_input, window, |this, _, event, _, cx| {
                match event {
                    InputEvent::PressEnter => this.search(cx),
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
            split_into_folders: false,
            find_debouncer: DebouncedDelay::new(),
            finding: false,
            find_input,
            local_result,
            global_result,
            active_items,
            active_subitems,
            focus_handle,
            scroll_handle,
            subscriptions,
        }
    }

    fn toggle_item(&mut self, item: Item, cx: &mut Context<Self>) {
        if !self.active_items.remove(&item) {
            self.active_items.insert(item);
        }
        cx.notify();
    }

    fn toggle_subitem(&mut self, subitem: SubItem, cx: &mut Context<Self>) {
        if !self.active_subitems.remove(&subitem) {
            self.active_subitems.insert(subitem);
        }
        cx.notify();
    }

    fn toggle_folder(&mut self, cx: &mut Context<Self>) {
        self.split_into_folders = !self.split_into_folders;
        cx.notify();
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
        let query = self.find_input.read(cx).text();

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
        let query = self.find_input.read(cx).text();
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

    fn push_room(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
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

    fn render_skeleton(&self, total: i32) -> impl IntoIterator<Item = impl IntoElement> {
        (0..total).map(|_| {
            div()
                .h_8()
                .w_full()
                .px_1()
                .flex()
                .items_center()
                .gap_2()
                .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                .child(Skeleton::new().w_20().h_3().rounded_sm())
        })
    }

    fn render_global_items(rooms: &[Entity<Room>], cx: &Context<Self>) -> Vec<FolderItem> {
        let mut items = Vec::with_capacity(rooms.len());

        for room in rooms.iter() {
            let this = room.read(cx);
            let id = this.id;
            let label = this.display_name(cx);
            let img = this.display_image(cx).map(img);

            let item = FolderItem::new(id as usize)
                .label(label)
                .img(img)
                .on_click({
                    cx.listener(move |this, _, window, cx| {
                        this.push_room(id, window, cx);
                    })
                });

            items.push(item);
        }

        items
    }

    fn render_items(rooms: &[Entity<Room>], cx: &Context<Self>) -> Vec<FolderItem> {
        let mut items = Vec::with_capacity(rooms.len());

        for room in rooms.iter() {
            let room = room.read(cx);
            let id = room.id;
            let ago = room.ago();
            let label = room.display_name(cx);
            let img = room.display_image(cx).map(img);

            let item = FolderItem::new(id as usize)
                .label(label)
                .description(ago)
                .img(img)
                .on_click({
                    cx.listener(move |_, _, window, cx| {
                        window.dispatch_action(
                            Box::new(AddPanel::new(PanelKind::Room(id), DockPlacement::Center)),
                            cx,
                        );
                    })
                });

            items.push(item);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let account = Account::get_global(cx).profile_ref();
        let registry = ChatRegistry::get_global(cx);

        // Get all rooms
        let rooms = registry.rooms(cx);
        let loading = registry.loading;

        // Get search result
        let local_result = self.local_result.read(cx);
        let global_result = self.global_result.read(cx);

        div()
            .id("sidebar")
            .track_focus(&self.focus_handle)
            .track_scroll(&self.scroll_handle)
            .overflow_y_scroll()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .py_1()
            .when_some(account, |this, profile| {
                this.child(
                    div()
                        .px_3()
                        .h_7()
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
                                .child(img(profile.shared_avatar()).size_7())
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
                        ),
                )
            })
            .child(
                div()
                    .px_3()
                    .h_7()
                    .flex_none()
                    .child(self.find_input.clone()),
            )
            .when_some(global_result.as_ref(), |this, rooms| {
                this.child(
                    div()
                        .px_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(Self::render_global_items(rooms, cx)),
                )
            })
            .child(
                div()
                    .px_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .mb_1()
                            .px_2()
                            .flex()
                            .justify_between()
                            .items_center()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().text_placeholder)
                            .child("Messages")
                            .child(
                                Button::new("menu")
                                    .tooltip("Toggle chat folders")
                                    .map(|this| {
                                        if self.split_into_folders {
                                            this.icon(IconName::FilterFill)
                                        } else {
                                            this.icon(IconName::Filter)
                                        }
                                    })
                                    .small()
                                    .custom(
                                        ButtonCustomVariant::new(window, cx)
                                            .foreground(cx.theme().text_placeholder)
                                            .color(cx.theme().ghost_element_background)
                                            .hover(cx.theme().ghost_element_background)
                                            .active(cx.theme().ghost_element_background),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_folder(cx);
                                    })),
                            ),
                    )
                    .when(loading, |this| this.children(self.render_skeleton(6)))
                    .map(|this| {
                        if let Some(rooms) = local_result {
                            this.children(Self::render_items(rooms, cx))
                        } else if !self.split_into_folders {
                            let rooms = rooms
                                .values()
                                .flat_map(|v| v.iter().cloned())
                                .sorted_by_key(|e| Reverse(e.read(cx).created_at))
                                .collect_vec();

                            this.children(Self::render_items(&rooms, cx))
                        } else {
                            let ongoing = rooms.get(&RoomKind::Ongoing);
                            let trusted = rooms.get(&RoomKind::Trusted);
                            let unknown = rooms.get(&RoomKind::Unknown);

                            this.when_some(ongoing, |this, rooms| {
                                this.child(
                                    Folder::new("Ongoing")
                                        .icon(IconName::Folder)
                                        .tooltip("All ongoing conversations")
                                        .collapsed(!self.active_items.contains(&Item::Ongoing))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_item(Item::Ongoing, cx);
                                        }))
                                        .children(Self::render_items(rooms, cx)),
                                )
                            })
                            .child(
                                Parent::new("Incoming")
                                    .icon(IconName::Folder)
                                    .tooltip("Incoming messages")
                                    .collapsed(!self.active_items.contains(&Item::Incoming))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_item(Item::Incoming, cx);
                                    }))
                                    .when_some(trusted, |this, rooms| {
                                        this.child(
                                            Folder::new("Trusted")
                                                .icon(IconName::Folder)
                                                .tooltip("Incoming messages from trusted contacts")
                                                .collapsed(
                                                    !self
                                                        .active_subitems
                                                        .contains(&SubItem::Trusted),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.toggle_subitem(SubItem::Trusted, cx);
                                                }))
                                                .children(Self::render_items(rooms, cx)),
                                        )
                                    })
                                    .when_some(unknown, |this, rooms| {
                                        this.child(
                                            Folder::new("Unknown")
                                                .icon(IconName::Folder)
                                                .tooltip("Incoming messages from unknowns")
                                                .collapsed(
                                                    !self
                                                        .active_subitems
                                                        .contains(&SubItem::Unknown),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.toggle_subitem(SubItem::Unknown, cx);
                                                }))
                                                .children(Self::render_items(rooms, cx)),
                                        )
                                    }),
                            )
                        }
                    }),
            )
    }
}
