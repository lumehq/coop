use crate::views::sidebar::inbox::Inbox;
use chat_state::registry::ChatRegistry;
use compose::Compose;
use gpui::{
    div, px, AnyElement, App, AppContext, BorrowAppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, ContextModal, Icon, IconName, Sizable, StyledExt,
};

mod compose;
mod inbox;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Sidebar> {
    Sidebar::new(window, cx)
}

pub struct Sidebar {
    // Panel
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Dock
    inbox: Entity<Inbox>,
}

impl Sidebar {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inbox = cx.new(|cx| Inbox::new(window, cx));

        Self {
            name: "Sidebar".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            inbox,
        }
    }

    fn show_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let compose = cx.new(|cx| Compose::new(window, cx));

        window.open_modal(cx, move |modal, window, cx| {
            let label = compose.read(cx).label(window, cx);

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
                            Button::new("create")
                                .label(label)
                                .primary()
                                .bold()
                                .rounded(ButtonRounded::Large)
                                .w_full()
                                .on_click(window.listener_for(&compose, |this, _, window, cx| {
                                    if let Some(room) = this.room(window, cx) {
                                        cx.update_global::<ChatRegistry, _>(|this, cx| {
                                            this.new_room(room, cx);
                                        });

                                        window.close_modal(cx);
                                    }
                                })),
                        ),
                )
        })
    }
}

impl Panel for Sidebar {
    fn panel_id(&self) -> SharedString {
        "Sidebar".into()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closable(&self, _cx: &App) -> bool {
        self.closable
    }

    fn zoomable(&self, _cx: &App) -> bool {
        self.zoomable
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
        v_flex()
            .w_full()
            .py_3()
            .gap_3()
            .child(
                v_flex().px_2().gap_1().child(
                    div()
                        .id("new")
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_1()
                        .h_7()
                        .text_xs()
                        .font_semibold()
                        .rounded(px(cx.theme().radius))
                        .child(
                            div()
                                .size_6()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(cx.theme().accent.step(cx, ColorScaleStep::NINE))
                                .child(
                                    Icon::new(IconName::ComposeFill)
                                        .small()
                                        .text_color(cx.theme().base.darken(cx)),
                                ),
                        )
                        .child("New Message")
                        .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
                        .on_click(cx.listener(|this, _, window, cx| this.show_compose(window, cx))),
                ),
            )
            .child(self.inbox.clone())
    }
}
