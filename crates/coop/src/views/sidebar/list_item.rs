use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, rems, App, ClickEvent, Div, InteractiveElement, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
use i18n::t;
use nostr_sdk::prelude::*;
use registry::room::RoomKind;
use registry::Registry;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::actions::OpenProfile;
use ui::avatar::Avatar;
use ui::context_menu::ContextMenuExt;
use ui::modal::ModalButtonProps;
use ui::{h_flex, ContextModal, StyledExt};

use crate::views::screening;

#[derive(IntoElement)]
pub struct RoomListItem {
    ix: usize,
    base: Div,
    room_id: u64,
    public_key: PublicKey,
    name: SharedString,
    avatar: SharedString,
    created_at: SharedString,
    kind: RoomKind,
    #[allow(clippy::type_complexity)]
    handler: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl RoomListItem {
    pub fn new(
        ix: usize,
        room_id: u64,
        public_key: PublicKey,
        name: SharedString,
        avatar: SharedString,
        created_at: SharedString,
        kind: RoomKind,
    ) -> Self {
        Self {
            ix,
            public_key,
            room_id,
            name,
            avatar,
            created_at,
            kind,
            base: h_flex().h_9().w_full().px_1p5(),
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Rc::new(handler);
        self
    }
}

impl RenderOnce for RoomListItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let public_key = self.public_key;
        let room_id = self.room_id;
        let kind = self.kind;
        let handler = self.handler.clone();
        let hide_avatar = AppSettings::get_hide_user_avatars(cx);
        let require_screening = AppSettings::get_screening(cx);

        self.base
            .id(self.ix)
            .gap_2()
            .text_sm()
            .rounded(cx.theme().radius)
            .when(!hide_avatar, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .size_6()
                        .rounded_full()
                        .overflow_hidden()
                        .child(Avatar::new(self.avatar).size(rems(1.5))),
                )
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_1()
                            .line_clamp(1)
                            .text_ellipsis()
                            .truncate()
                            .font_medium()
                            .child(self.name),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(cx.theme().text_placeholder)
                            .child(self.created_at),
                    ),
            )
            .context_menu(move |this, _window, _cx| {
                // TODO: add share chat room
                this.menu(t!("profile.view"), Box::new(OpenProfile(public_key)))
            })
            .hover(|this| this.bg(cx.theme().elevated_surface_background))
            .on_click(move |event, window, cx| {
                handler(event, window, cx);

                if kind != RoomKind::Ongoing && require_screening {
                    let screening = screening::init(public_key, window, cx);

                    window.open_modal(cx, move |this, _window, _cx| {
                        this.confirm()
                            .child(screening.clone())
                            .button_props(
                                ModalButtonProps::default()
                                    .cancel_text(t!("screening.ignore"))
                                    .ok_text(t!("screening.response")),
                            )
                            .on_cancel(move |_event, _window, cx| {
                                Registry::global(cx).update(cx, |this, cx| {
                                    this.close_room(room_id, cx);
                                });
                                // false to prevent closing the modal
                                // modal will be closed after closing panel
                                false
                            })
                    });
                }
            })
    }
}
