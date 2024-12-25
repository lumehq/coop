use coop_ui::{
    button::Button,
    dock::{Panel, PanelEvent, PanelState},
    popup_menu::PopupMenu,
};
use gpui::*;
use room::ChatRoom;
use std::sync::Arc;

use crate::states::chat::Room;

mod room;

pub struct ChatPanel {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Room
    id: SharedString,
    room: View<ChatRoom>,
}

impl ChatPanel {
    pub fn new(room: &Arc<Room>, cx: &mut WindowContext) -> View<Self> {
        let id = room.id.clone();
        let room = cx.new_view(|cx| {
            let view = ChatRoom::new(room, cx);
            // Load messages
            view.load(cx);
            // Subscribe for new messages
            view.subscribe(cx);

            view
        });

        cx.new_view(|cx| Self {
            name: "Chat".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            id,
            room,
        })
    }
}

impl Panel for ChatPanel {
    fn panel_name(&self) -> SharedString {
        self.id.clone()
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

impl EventEmitter<PanelEvent> for ChatPanel {}

impl FocusableView for ChatPanel {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatPanel {
    fn render(&mut self, _cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        div().size_full().child(self.room.clone())
    }
}
