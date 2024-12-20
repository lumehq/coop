use coop_ui::{
    button::Button,
    dock::{Panel, PanelEvent, PanelState, TitleStyle},
    popup_menu::PopupMenu,
    v_flex,
};
use form::Form;
use gpui::*;
use list::MessageList;
use nostr_sdk::*;

pub mod form;
pub mod list;

pub struct ChatPanel {
    // Panel
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    list: View<MessageList>,
    form: View<Form>,
}

impl ChatPanel {
    pub fn new(from: PublicKey, cx: &mut WindowContext) -> View<Self> {
        let form = cx.new_view(|cx| Form::new(from, cx));
        let list = cx.new_view(|cx| {
            let list = MessageList::new(from, cx);
            // Load messages from database
            list.init(cx);
            // Subscribe for new message
            list.subscribe(cx);

            list
        });

        cx.new_view(|cx| Self {
            name: "Chat".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            list,
            form,
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
        v_flex()
            .size_full()
            .child(div().flex_1().min_h_0().child(self.list.clone()))
            .child(self.form.clone())
    }
}
