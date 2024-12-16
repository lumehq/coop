use coop_ui::{
    button::Button,
    button_group::ButtonGroup,
    dock::{DockItemState, Panel, PanelEvent, TitleStyle},
    input::TextInput,
    popup_menu::PopupMenu,
    Sizable,
};
use gpui::*;
use messages::Messages;
use nostr_sdk::*;

pub mod messages;

pub struct ChatPanel {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    messages: View<Messages>,
    input: View<TextInput>,
}

impl ChatPanel {
    pub fn new(from: PublicKey, cx: &mut WindowContext) -> View<Self> {
        let input = cx.new_view(TextInput::new);
        let messages = cx.new_view(|cx| Messages::new(from, cx));

        input.update(cx, |input, _cx| {
            input.set_placeholder("Message");
        });

        cx.new_view(|cx| Self {
            name: "Chat".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            messages,
            input,
        })
    }
}

impl Panel for ChatPanel {
    fn panel_name(&self) -> &'static str {
        "ChatPanel"
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn title_style(&self, _cx: &WindowContext) -> Option<TitleStyle> {
        None
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

    fn dump(&self, _cx: &AppContext) -> DockItemState {
        DockItemState::new(self)
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
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.messages.clone())
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_11()
                    .child(self.input.clone())
                    .child(
                        ButtonGroup::new("actions")
                            .large()
                            .child(Button::new("upload").label("Upload"))
                            .child(Button::new("send").label("Send")),
                    ),
            )
    }
}
