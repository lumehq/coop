use std::{cmp::Reverse, collections::HashSet};

use account::Account;
use button::SidebarButton;
use chats::{
    room::{Room, RoomKind},
    ChatRegistry,
};
use common::profile::SharedProfile;
use folder::{Folder, FolderItem, Parent};
use global::get_client;
use gpui::{
    actions, div, img, prelude::FluentBuilder, AnyElement, App, AppContext, Context, Entity,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Task, Window,
};
use itertools::Itertools;
use theme::ActiveTheme;
use ui::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    dock_area::{
        dock::DockPlacement,
        panel::{Panel, PanelEvent},
    },
    popup_menu::{PopupMenu, PopupMenuExt},
    skeleton::Skeleton,
    IconName, Sizable, StyledExt,
};

use crate::chatspace::{AddPanel, ModalKind, PanelKind, ToggleModal};

mod button;
mod folder;

actions!(profile, [Logout]);

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
    split_into_folders: bool,
    active_items: HashSet<Item>,
    active_subitems: HashSet<SubItem>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let scroll_handle = ScrollHandle::default();

        let mut active_items = HashSet::with_capacity(2);
        active_items.insert(Item::Ongoing);

        let mut active_subitems = HashSet::with_capacity(2);
        active_subitems.insert(SubItem::Trusted);
        active_subitems.insert(SubItem::Unknown);

        Self {
            name: "Chat Sidebar".into(),
            split_into_folders: false,
            active_items,
            active_subitems,
            focus_handle,
            scroll_handle,
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

    fn split_into_folders(&mut self, cx: &mut Context<Self>) {
        self.split_into_folders = !self.split_into_folders;
        cx.notify();
    }

    fn on_logout(&mut self, _: &Logout, window: &mut Window, cx: &mut Context<Self>) {
        let task: Task<Result<(), anyhow::Error>> = cx.background_spawn(async move {
            let client = get_client();
            _ = client.reset().await;

            Ok(())
        });

        cx.spawn_in(window, async move |_, cx| {
            if task.await.is_ok() {
                cx.update(|_, cx| {
                    Account::global(cx).update(cx, |this, cx| {
                        this.profile = None;
                        cx.notify();
                    });
                })
                .ok();
            };
        })
        .detach();
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

    fn render_items(rooms: &Vec<&Entity<Room>>, cx: &Context<Self>) -> Vec<FolderItem> {
        let mut items = Vec::with_capacity(rooms.len());

        for room in rooms {
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
        let account = Account::global(cx).read(cx).profile.as_ref();
        let registry = ChatRegistry::global(cx).read(cx);

        let rooms = registry.rooms(cx);
        let loading = registry.loading();

        div()
            .id("sidebar")
            .track_focus(&self.focus_handle)
            .track_scroll(&self.scroll_handle)
            .on_action(cx.listener(Self::on_logout))
            .overflow_y_scroll()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .pt_1()
            .px_2()
            .pb_2()
            .when_some(account, |this, profile| {
                this.child(
                    div()
                        .h_7()
                        .px_1p5()
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
                            Button::new("user_dropdown")
                                .icon(IconName::Ellipsis)
                                .small()
                                .ghost()
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
                                    .separator()
                                    .menu("Logout", Box::new(Logout))
                                }),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .font_medium()
                    .child(
                        SidebarButton::new("Find")
                            .icon(IconName::Search)
                            .on_click(cx.listener(|_, _, window, cx| {
                                window.dispatch_action(
                                    Box::new(ToggleModal {
                                        modal: ModalKind::Search,
                                    }),
                                    cx,
                                );
                            })),
                    )
                    .child(
                        SidebarButton::new("New Chat")
                            .icon(IconName::PlusCircleFill)
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .pl_2()
                            .pr_1()
                            .flex()
                            .justify_between()
                            .items_center()
                            .text_xs()
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
                                        this.split_into_folders(cx);
                                    })),
                            ),
                    )
                    .map(|this| {
                        if loading {
                            this.children(self.render_skeleton(6))
                        } else if !self.split_into_folders {
                            let rooms: Vec<_> = rooms
                                .values()
                                .flat_map(|v| v.iter().cloned())
                                .sorted_by_key(|e| Reverse(e.read(cx).created_at))
                                .collect();

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
