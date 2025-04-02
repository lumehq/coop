use chats::ChatRegistry;
use compose::{Compose, ComposeButton};
use folder::{Folder, FolderItem};
use gpui::{
    div, img, prelude::FluentBuilder, px, relative, AnyElement, App, AppContext, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement, Render, SharedString, Styled,
    Window,
};
use header::Header;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    skeleton::Skeleton,
    theme::{scale::ColorScaleStep, ActiveTheme},
    Collapsible, ContextModal, Disableable, IconName, StyledExt,
};

use crate::chat_space::{AddPanel, PanelKind};

mod compose;
mod folder;
mod header;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    Sidebar::new(window, cx)
}

pub struct Sidebar {
    name: SharedString,
    focus_handle: FocusHandle,
    collapsed: bool,
    inbox_collapsed: bool,
    verified_collapsed: bool,
    other_collapsed: bool,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        Self {
            name: "Sidebar".into(),
            focus_handle,
            collapsed: false,
            inbox_collapsed: false,
            verified_collapsed: true,
            other_collapsed: true,
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

    fn collapse(&mut self, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        cx.notify();
    }

    fn inbox_collapse(&mut self, cx: &mut Context<Self>) {
        self.inbox_collapsed = !self.inbox_collapsed;
        cx.notify();
    }

    fn verified_collapse(&mut self, cx: &mut Context<Self>) {
        self.verified_collapsed = !self.verified_collapsed;
        cx.notify();
    }

    fn other_collapse(&mut self, cx: &mut Context<Self>) {
        self.other_collapsed = !self.other_collapsed;
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
        div()
            .scrollable(cx.entity_id(), ScrollbarAxis::Vertical)
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .px_2()
                    .py_3()
                    .w_full()
                    .child(ComposeButton::new("New Message").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.render_compose(window, cx);
                        },
                    ))),
            )
            .child(
                div()
                    .px_2()
                    .w_full()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        Header::new("Chat Folders", IconName::BubbleFill)
                            .collapsed(self.collapsed)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.collapse(cx);
                            })),
                    )
                    .when(!self.collapsed, |this| {
                        this.map(|this| {
                            let state = ChatRegistry::global(cx);
                            let is_loading = state.read(cx).is_loading();

                            if is_loading {
                                this.children(self.render_skeleton(5))
                            } else if state.read(cx).rooms().is_empty() {
                                this.child(
                                    div()
                                        .px_1()
                                        .w_full()
                                        .h_20()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .text_center()
                                        .rounded(px(cx.theme().radius))
                                        .bg(cx.theme().base.step(cx, ColorScaleStep::THREE))
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .line_height(relative(1.2))
                                                .child("No chats"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(
                                                    cx.theme()
                                                        .base
                                                        .step(cx, ColorScaleStep::ELEVEN),
                                                )
                                                .child("Recent chats will appear here."),
                                        ),
                                )
                            } else {
                                let inbox = state.read(cx).inbox_rooms(cx);
                                let verified = state.read(cx).verified_rooms(cx);
                                let others = state.read(cx).other_rooms(cx);

                                this.child(
                                    Folder::new("Inbox")
                                        .icon(IconName::FolderFill)
                                        .collapsed(self.inbox_collapsed)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.inbox_collapse(cx);
                                        }))
                                        .children({
                                            let mut items = vec![];

                                            for room in inbox {
                                                let room = room.read(cx);
                                                let ago = room.last_seen().ago();
                                                let Some(member) = room.first_member() else {
                                                    continue;
                                                };

                                                let label = if room.is_group() {
                                                    room.name().unwrap_or("Unnamed".into())
                                                } else {
                                                    member.name.clone()
                                                };

                                                let img = if !room.is_group() {
                                                    Some(img(member.avatar.clone()))
                                                } else {
                                                    None
                                                };

                                                let item = FolderItem::new(label, ago)
                                                    .img(img)
                                                    .on_click({
                                                        let id = room.id;

                                                        cx.listener(move |this, _, window, cx| {
                                                            this.open_room(id, window, cx);
                                                        })
                                                    });

                                                items.push(item);
                                            }

                                            items
                                        }),
                                )
                                .child(
                                    Folder::new("Verified")
                                        .icon(IconName::FolderFill)
                                        .collapsed(self.verified_collapsed)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.verified_collapse(cx);
                                        }))
                                        .children({
                                            let mut items = vec![];

                                            for room in verified {
                                                let room = room.read(cx);
                                                let ago = room.last_seen().ago();
                                                let Some(member) = room.first_member() else {
                                                    continue;
                                                };

                                                let label = if room.is_group() {
                                                    room.name().unwrap_or("Unnamed".into())
                                                } else {
                                                    member.name.clone()
                                                };

                                                let img = if !room.is_group() {
                                                    Some(img(member.avatar.clone()))
                                                } else {
                                                    None
                                                };

                                                let item = FolderItem::new(label, ago)
                                                    .img(img)
                                                    .on_click({
                                                        let id = room.id;

                                                        cx.listener(move |this, _, window, cx| {
                                                            this.open_room(id, window, cx);
                                                        })
                                                    });

                                                items.push(item);
                                            }

                                            items
                                        }),
                                )
                                .child(
                                    Folder::new("Others")
                                        .icon(IconName::FolderFill)
                                        .collapsed(self.other_collapsed)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.other_collapse(cx);
                                        }))
                                        .children({
                                            let mut items = vec![];

                                            for room in others {
                                                let room = room.read(cx);
                                                let ago = room.last_seen().ago();
                                                let Some(member) = room.first_member() else {
                                                    continue;
                                                };

                                                let label = if room.is_group() {
                                                    room.name().unwrap_or("Unnamed".into())
                                                } else {
                                                    member.name.clone()
                                                };

                                                let img = if !room.is_group() {
                                                    Some(img(member.avatar.clone()))
                                                } else {
                                                    None
                                                };

                                                let item = FolderItem::new(label, ago)
                                                    .img(img)
                                                    .on_click({
                                                        let id = room.id;

                                                        cx.listener(move |this, _, window, cx| {
                                                            this.open_room(id, window, cx);
                                                        })
                                                    });

                                                items.push(item);
                                            }

                                            items
                                        }),
                                )
                            }
                        })
                    }),
            )
    }
}
