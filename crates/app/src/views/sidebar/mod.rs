use chats::{
    room::{Room, RoomKind},
    ChatRegistry,
};
use compose::{Compose, ComposeButton};
use folder::{Folder, FolderItem, Parent};
use gpui::{
    div, img, prelude::FluentBuilder, px, AnyElement, App, AppContext, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement, Render, SharedString, Styled,
    Window,
};
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    skeleton::Skeleton,
    theme::{scale::ColorScaleStep, ActiveTheme},
    ContextModal, Disableable, IconName, StyledExt,
};

use crate::chat_space::{AddPanel, PanelKind};

mod compose;
mod folder;

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

    fn render_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let compose = cx.new(|cx| Compose::new(window, cx));

        window.open_modal(cx, move |modal, window, cx| {
            let label = compose.read(cx).label(window, cx);
            let is_submitting = compose.read(cx).is_submitting();

            modal
                .title("Direct Messages")
                .width(px(420.))
                .child(compose.clone())
                .footer(
                    div()
                        .p_2()
                        .border_t_1()
                        .border_color(cx.theme().base.step(cx, ColorScaleStep::FIVE))
                        .child(
                            Button::new("create_dm_btn")
                                .label(label)
                                .primary()
                                .bold()
                                .rounded(ButtonRounded::Large)
                                .w_full()
                                .loading(is_submitting)
                                .disabled(is_submitting)
                                .on_click(window.listener_for(&compose, |this, _, window, cx| {
                                    this.compose(window, cx)
                                })),
                        ),
                )
        })
    }

    fn open_room(&self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(
            Box::new(AddPanel::new(
                PanelKind::Room(id),
                ui::dock_area::dock::DockPlacement::Center,
            )),
            cx,
        );
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

    #[allow(dead_code)]
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
            let ago = room.last_seen.ago();
            let label = room.display_name(cx);
            let img = room.display_image(cx).map(img);

            let item = FolderItem::new(id as usize)
                .label(label)
                .description(ago)
                .img(img)
                .on_click({
                    cx.listener(move |this, _, window, cx| {
                        this.open_room(id, window, cx);
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let registry = ChatRegistry::global(cx).read(cx);
        let rooms = registry.rooms(cx);
        let loading = registry.loading();

        let ongoing = rooms.get(&RoomKind::Ongoing);
        let trusted = rooms.get(&RoomKind::Trusted);
        let unknown = rooms.get(&RoomKind::Unknown);

        div()
            .scrollable(cx.entity_id(), ScrollbarAxis::Vertical)
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .px_2()
            .py_3()
            .child(ComposeButton::new("New Message").on_click(cx.listener(
                |this, _, window, cx| {
                    this.render_compose(window, cx);
                },
            )))
            .map(|this| {
                if loading {
                    this.children(self.render_skeleton(6))
                } else {
                    this.when_some(ongoing, |this, rooms| {
                        this.child(
                            Folder::new("Ongoing")
                                .icon(IconName::FolderFill)
                                .active_icon(IconName::FolderOpenFill)
                                .collapsed(self.ongoing)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.ongoing(cx);
                                }))
                                .children(Self::render_items(rooms, cx)),
                        )
                    })
                    .child(
                        Parent::new("Incoming")
                            .icon(IconName::FolderFill)
                            .active_icon(IconName::FolderOpenFill)
                            .collapsed(self.incoming)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.incoming(cx);
                            }))
                            .when_some(trusted, |this, rooms| {
                                this.child(
                                    Folder::new("Trusted")
                                        .icon(IconName::FolderFill)
                                        .active_icon(IconName::FolderOpenFill)
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
                                        .icon(IconName::FolderFill)
                                        .active_icon(IconName::FolderOpenFill)
                                        .collapsed(self.unknown)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.unknown(cx);
                                        }))
                                        .children(Self::render_items(rooms, cx)),
                                )
                            }),
                    )
                }
            })
    }
}
