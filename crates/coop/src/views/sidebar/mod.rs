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
    SharedString, Styled, Task, Window,
};
use ui::{
    button::{Button, ButtonVariants},
    dock_area::{
        dock::DockPlacement,
        panel::{Panel, PanelEvent},
    },
    popup_menu::{PopupMenu, PopupMenuExt},
    scroll::ScrollbarAxis,
    skeleton::Skeleton,
    theme::{scale::ColorScaleStep, ActiveTheme},
    IconName, Sizable, StyledExt,
};

use crate::chat_space::{AddPanel, ModalKind, PanelKind, ToggleModal};

mod button;
mod folder;

actions!(profile, [Logout]);

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    Sidebar::new(window, cx)
}

pub struct Sidebar {
    name: SharedString,
    focus_handle: FocusHandle,
    ongoing: bool,
    incoming: bool,
    trusted: bool,
    unknown: bool,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        Self {
            name: "Chat Sidebar".into(),
            ongoing: false,
            incoming: false,
            trusted: true,
            unknown: true,
            focus_handle,
        }
    }

    fn ongoing(&mut self, cx: &mut Context<Self>) {
        self.ongoing = !self.ongoing;
        cx.notify();
    }

    fn incoming(&mut self, cx: &mut Context<Self>) {
        self.incoming = !self.incoming;
        cx.notify();
    }

    fn trusted(&mut self, cx: &mut Context<Self>) {
        self.trusted = !self.trusted;
        cx.notify();
    }

    fn unknown(&mut self, cx: &mut Context<Self>) {
        self.unknown = !self.unknown;
        cx.notify();
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
        let account = Account::global(cx).read(cx).profile.as_ref();
        let registry = ChatRegistry::global(cx).read(cx);

        let rooms = registry.rooms(cx);
        let loading = registry.loading();

        let ongoing = rooms.get(&RoomKind::Ongoing);
        let trusted = rooms.get(&RoomKind::Trusted);
        let unknown = rooms.get(&RoomKind::Unknown);

        div()
            .scrollable(cx.entity_id(), ScrollbarAxis::Vertical)
            .on_action(cx.listener(Self::on_logout))
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
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .child(
                        SidebarButton::new("New Message")
                            .icon(IconName::PlusCircleFill)
                            .on_click(cx.listener(|_, _, window, cx| {
                                window.dispatch_action(
                                    Box::new(ToggleModal {
                                        modal: ModalKind::Compose,
                                    }),
                                    cx,
                                );
                            })),
                    )
                    .child(
                        SidebarButton::new("Contacts")
                            .icon(IconName::AddressBook)
                            .on_click(cx.listener(|_, _, window, cx| {
                                window.dispatch_action(
                                    Box::new(ToggleModal {
                                        modal: ModalKind::Contact,
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
                            .px_2()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().base.step(cx, ColorScaleStep::NINE))
                            .child("Messages"),
                    )
                    .map(|this| {
                        if loading {
                            this.children(self.render_skeleton(6))
                        } else {
                            this.when_some(ongoing, |this, rooms| {
                                this.child(
                                    Folder::new("Ongoing")
                                        .icon(IconName::Folder)
                                        .collapsed(self.ongoing)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.ongoing(cx);
                                        }))
                                        .children(Self::render_items(rooms, cx)),
                                )
                            })
                            .child(
                                Parent::new("Incoming")
                                    .icon(IconName::Folder)
                                    .collapsed(self.incoming)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.incoming(cx);
                                    }))
                                    .when_some(trusted, |this, rooms| {
                                        this.child(
                                            Folder::new("Trusted")
                                                .icon(IconName::Folder)
                                                .collapsed(self.trusted)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.trusted(cx);
                                                }))
                                                .children(Self::render_items(rooms, cx)),
                                        )
                                    })
                                    .when_some(unknown, |this, rooms| {
                                        this.child(
                                            Folder::new("Unknown")
                                                .icon(IconName::Folder)
                                                .collapsed(self.unknown)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.unknown(cx);
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
