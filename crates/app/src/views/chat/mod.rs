use gpui::{
    div, AnyElement, AppContext, EventEmitter, FocusHandle, FocusableView, IntoElement,
    ParentElement, Render, SharedString, Styled, View, VisualContext, WindowContext,
};
use nostr_sdk::prelude::*;
use room::RoomPanel;
use std::sync::Arc;
use ui::{
    button::Button,
    dock::{Panel, PanelEvent, PanelState},
    popup_menu::PopupMenu,
};

use crate::states::chat::Room;

mod message;
mod room;

pub struct ChatPanel {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Room
    id: SharedString,
    room: View<RoomPanel>,
    metadata: Option<Metadata>,
}

impl ChatPanel {
    pub fn new(room: &Arc<Room>, cx: &mut WindowContext) -> View<Self> {
        let id = room.id.clone();
        let title = room.title.clone();
        let metadata = room.metadata.clone();

        let room = cx.new_view(|cx| {
            let view = RoomPanel::new(room, cx);
            // Load messages
            view.load(cx);
            // Subscribe for new messages
            view.subscribe(cx);

            view
        });

        cx.new_view(|cx| Self {
            name: title.unwrap_or("Untitled".into()),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            id,
            room,
            metadata,
        })
    }
}

impl Panel for ChatPanel {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn panel_metadata(&self) -> Option<Metadata> {
        self.metadata.clone()
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
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatPanel {
    fn render(&mut self, _cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        div().size_full().child(self.room.clone())
    }
}
