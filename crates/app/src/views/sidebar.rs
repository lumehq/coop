use coop_ui::{
    button::{Button, ButtonVariants},
    dock::{Panel, PanelEvent, PanelState},
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    v_flex, ContextModal, Icon, IconName, Sizable, StyledExt,
};
use gpui::*;

use super::inbox::Inbox;
use crate::views::app::{AddPanel, PanelKind};

pub struct Sidebar {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Dock
    inbox: View<Inbox>,
    view_id: EntityId,
}

impl Sidebar {
    pub fn new(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::view)
    }

    fn view(cx: &mut ViewContext<Self>) -> Self {
        let inbox = cx.new_view(Inbox::new);

        Self {
            name: "Left Dock".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            view_id: cx.view().entity_id(),
            inbox,
        }
    }
}

impl Panel for Sidebar {
    fn panel_id(&self) -> SharedString {
        "Sidebar".into()
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closeable(&self, _cx: &WindowContext) -> bool {
        self.closeable
    }

    fn zoomable(&self, _cx: &WindowContext) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &WindowContext) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _cx: &WindowContext) -> Vec<Button> {
        vec![]
    }

    fn dump(&self, _cx: &AppContext) -> PanelState {
        PanelState::new(self)
    }
}

impl EventEmitter<PanelEvent> for Sidebar {}

impl FocusableView for Sidebar {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .scrollable(self.view_id, ScrollbarAxis::Vertical)
            .pt_3()
            .gap_3()
            .child(
                v_flex()
                    .px_2()
                    .gap_1()
                    .child(
                        Button::new("new")
                            .small()
                            .ghost()
                            .not_centered()
                            .bold()
                            .icon(Icon::new(IconName::Plus))
                            .label("New")
                            .on_click(|_, cx| {
                                cx.open_modal(move |modal, _| modal.child("TODO"));
                            }),
                    )
                    .child(
                        Button::new("contacts")
                            .small()
                            .ghost()
                            .not_centered()
                            .bold()
                            .icon(Icon::new(IconName::Group))
                            .label("Contacts")
                            .on_click(|_, cx| {
                                cx.dispatch_action(Box::new(AddPanel {
                                    panel: PanelKind::Contact,
                                    position: coop_ui::dock::DockPlacement::Center,
                                }))
                            }),
                    ),
            )
            .child(self.inbox.clone())
    }
}
