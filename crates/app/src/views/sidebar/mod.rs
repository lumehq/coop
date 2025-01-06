use std::sync::Arc;

use contact_list::ContactList;
use gpui::*;
use nostr_sdk::Timestamp;
use rnglib::{Language, RNG};
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock::{Panel, PanelEvent, PanelState},
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    v_flex, ContextModal, Icon, IconName, Sizable, StyledExt,
};

use crate::states::{account::AccountRegistry, chat::Room};

use super::{
    app::{AddPanel, PanelKind},
    inbox::Inbox,
};

mod contact_list;

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

    fn show_compose(&mut self, cx: &mut ViewContext<Self>) {
        let contact_list = cx.new_view(ContactList::new);

        cx.open_modal(move |modal, _cx| {
            modal.child(contact_list.clone()).footer(
                div().flex().gap_2().child(
                    Button::new("create")
                        .label("Create DM")
                        .primary()
                        .rounded(ButtonRounded::Large)
                        .w_full()
                        .on_click({
                            let contact_list = contact_list.clone();
                            move |_, cx| {
                                let members = contact_list.model.read(cx).selected();
                                let owner = cx.global::<AccountRegistry>().get().unwrap();
                                let rng = RNG::from(&Language::Roman);
                                let name = rng.generate_names(2, true).join("-").to_lowercase();

                                let room = Arc::new(Room::new(
                                    owner,
                                    members,
                                    Timestamp::now(),
                                    Some(name.into()),
                                    cx,
                                ));

                                cx.dispatch_action(Box::new(AddPanel {
                                    panel: PanelKind::Room(room),
                                    position: ui::dock::DockPlacement::Center,
                                }))
                            }
                        }),
                ),
            )
        })
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
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .scrollable(self.view_id, ScrollbarAxis::Vertical)
            .py_3()
            .gap_3()
            .child(
                v_flex()
                    .px_2()
                    .gap_1()
                    .child(
                        Button::new("compose")
                            .small()
                            .ghost()
                            .not_centered()
                            .icon(Icon::new(IconName::ComposeFill))
                            .label("New Message")
                            .on_click(cx.listener(|this, _, cx| this.show_compose(cx))),
                    )
                    .child(
                        Button::new("contacts")
                            .small()
                            .ghost()
                            .not_centered()
                            .icon(Icon::new(IconName::GroupFill))
                            .label("Contacts"),
                    ),
            )
            .child(self.inbox.clone())
    }
}
