use std::rc::Rc;

use chat::{ChatRegistry, RoomKind};
use chat_ui::{CopyPublicKey, OpenPublicKey};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, rems, App, ClickEvent, InteractiveElement, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use nostr_sdk::prelude::*;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::context_menu::ContextMenuExt;
use ui::modal::ModalButtonProps;
use ui::skeleton::Skeleton;
use ui::{h_flex, ContextModal, StyledExt};

use crate::views::screening;

#[derive(IntoElement)]
pub struct RoomListItem {
    ix: usize,
    room_id: Option<u64>,
    public_key: Option<PublicKey>,
    name: Option<SharedString>,
    avatar: Option<SharedString>,
    created_at: Option<SharedString>,
    kind: Option<RoomKind>,
    #[allow(clippy::type_complexity)]
    handler: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl RoomListItem {
    pub fn new(ix: usize) -> Self {
        Self {
            ix,
            room_id: None,
            public_key: None,
            name: None,
            avatar: None,
            created_at: None,
            kind: None,
            handler: None,
        }
    }

    pub fn room_id(mut self, room_id: u64) -> Self {
        self.room_id = Some(room_id);
        self
    }

    pub fn public_key(mut self, public_key: PublicKey) -> Self {
        self.public_key = Some(public_key);
        self
    }

    pub fn name(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn avatar(mut self, avatar: impl Into<SharedString>) -> Self {
        self.avatar = Some(avatar.into());
        self
    }

    pub fn created_at(mut self, created_at: impl Into<SharedString>) -> Self {
        self.created_at = Some(created_at.into());
        self
    }

    pub fn kind(mut self, kind: RoomKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for RoomListItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let settings = AppSettings::settings(cx);
        let hide_avatar = settings.hide_avatar;
        let require_screening = settings.screening.is_everyone();

        let (
            Some(public_key),
            Some(room_id),
            Some(name),
            Some(avatar),
            Some(created_at),
            Some(kind),
            Some(handler),
        ) = (
            self.public_key,
            self.room_id,
            self.name,
            self.avatar,
            self.created_at,
            self.kind,
            self.handler,
        )
        else {
            return h_flex()
                .id(self.ix)
                .h_9()
                .w_full()
                .px_1p5()
                .gap_2()
                .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_between()
                        .child(Skeleton::new().w_32().h_2p5().rounded(cx.theme().radius))
                        .child(Skeleton::new().w_6().h_2p5().rounded(cx.theme().radius)),
                );
        };

        h_flex()
            .id(self.ix)
            .h_9()
            .w_full()
            .px_1p5()
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
                        .child(Avatar::new(avatar).size(rems(1.5))),
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
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(cx.theme().text_placeholder)
                            .child(created_at),
                    ),
            )
            .hover(|this| this.bg(cx.theme().elevated_surface_background))
            .context_menu(move |this, _window, _cx| {
                this.menu("View Profile", Box::new(OpenPublicKey(public_key)))
                    .menu("Copy Public Key", Box::new(CopyPublicKey(public_key)))
            })
            .on_click(move |event, window, cx| {
                handler(event, window, cx);

                if kind != RoomKind::Ongoing && require_screening {
                    let screening = screening::init(public_key, window, cx);

                    window.open_modal(cx, move |this, _window, _cx| {
                        this.confirm()
                            .child(screening.clone())
                            .button_props(
                                ModalButtonProps::default()
                                    .cancel_text("Ignore")
                                    .ok_text("Response"),
                            )
                            .on_cancel(move |_event, _window, cx| {
                                ChatRegistry::global(cx).update(cx, |this, cx| {
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
