use std::time::Duration;

use gpui::*;
use item::ContactListItem;
use prelude::FluentBuilder;
use ui::{
    button::Button,
    dock::{Panel, PanelEvent, PanelState},
    popup_menu::PopupMenu,
    scroll::ScrollbarAxis,
    v_flex, StyledExt,
};

use crate::get_client;

mod item;

pub struct ContactPanel {
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Contacts
    view_id: EntityId,
    contacts: Model<Option<Vec<View<ContactListItem>>>>,
}

impl ContactPanel {
    pub fn new(cx: &mut WindowContext) -> View<Self> {
        cx.new_view(Self::view)
    }

    fn view(cx: &mut ViewContext<Self>) -> Self {
        let contacts = cx.new_model(|_| None);
        let async_contacts = contacts.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();

                async move {
                    if let Ok(contacts) = async_cx
                        .background_executor()
                        .spawn(async move { client.get_contact_list(Duration::from_secs(3)).await })
                        .await
                    {
                        let views: Vec<View<ContactListItem>> = contacts
                            .into_iter()
                            .map(|contact| {
                                async_cx
                                    .new_view(|cx| ContactListItem::new(contact.public_key, cx))
                                    .unwrap()
                            })
                            .collect();

                        _ = async_cx.update_model(&async_contacts, |model, cx| {
                            *model = Some(views);
                            cx.notify();
                        });
                    }
                }
            })
            .detach();

        Self {
            name: "Contacts".into(),
            closeable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
            view_id: cx.entity_id(),
            contacts,
        }
    }
}

impl Panel for ContactPanel {
    fn panel_id(&self) -> SharedString {
        "Contact".into()
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

impl EventEmitter<PanelEvent> for ContactPanel {}

impl FocusableView for ContactPanel {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ContactPanel {
    fn render(&mut self, cx: &mut gpui::ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .scrollable(self.view_id, ScrollbarAxis::Vertical)
            .w_full()
            .gap_1()
            .p_2()
            .when_some(self.contacts.read(cx).as_ref(), |this, contacts| {
                this.children(contacts.clone())
            })
    }
}
