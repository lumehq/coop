use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, relative, rems, App, ClickEvent, Div, InteractiveElement, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
use i18n::t;
use nostr_sdk::prelude::*;
use registry::room::RoomKind;
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
    public_key: PublicKey,
    name: Option<SharedString>,
    avatar: Option<SharedString>,
    created_at: Option<SharedString>,
    kind: Option<RoomKind>,
    #[allow(clippy::type_complexity)]
    handler: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl RoomListItem {
    pub fn new(ix: usize, public_key: PublicKey) -> Self {
        Self {
            ix,
            public_key,
            base: h_flex().h_9().w_full().px_1p5(),
            name: None,
            avatar: None,
            created_at: None,
            kind: None,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn name(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn created_at(mut self, created_at: impl Into<SharedString>) -> Self {
        self.created_at = Some(created_at.into());
        self
    }

    pub fn avatar(mut self, avatar: impl Into<SharedString>) -> Self {
        self.avatar = Some(avatar.into());
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
        self.handler = Rc::new(handler);
        self
    }
}

impl RenderOnce for RoomListItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let public_key = self.public_key;
        let kind = self.kind;
        let handler = self.handler.clone();
        let hide_avatar = AppSettings::get_global(cx).settings.hide_user_avatars;
        let screening = AppSettings::get_global(cx).settings.screening;

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
                        .map(|this| {
                            if let Some(img) = self.avatar {
                                this.child(Avatar::new(img).size(rems(1.5)))
                            } else {
                                this.child(
                                    img("brand/avatar.png")
                                        .rounded_full()
                                        .size_6()
                                        .into_any_element(),
                                )
                            }
                        }),
                )
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when_some(self.name, |this, name| {
                        this.child(
                            div()
                                .flex_1()
                                .line_clamp(1)
                                .text_ellipsis()
                                .truncate()
                                .font_medium()
                                .child(name),
                        )
                    })
                    .when_some(self.created_at, |this, ago| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().text_placeholder)
                                .child(ago),
                        )
                    }),
            )
            .context_menu(move |this, _window, _cx| {
                // TODO: add share chat room
                this.menu(t!("profile.view"), Box::new(OpenProfile(public_key)))
            })
            .hover(|this| this.bg(cx.theme().elevated_surface_background))
            .on_click(move |event, window, cx| {
                let handler = handler.clone();

                if let Some(kind) = kind {
                    if kind != RoomKind::Ongoing && screening {
                        let screening = screening::init(public_key, window, cx);

                        window.open_modal(cx, move |this, _window, _cx| {
                            let handler_clone = handler.clone();

                            this.confirm()
                                .child(screening.clone())
                                .button_props(
                                    ModalButtonProps::default()
                                        .cancel_text(t!("screening.ignore"))
                                        .ok_text(t!("screening.response")),
                                )
                                .on_ok(move |event, window, cx| {
                                    handler_clone(event, window, cx);
                                    // true to close the modal
                                    true
                                })
                        });
                    } else {
                        handler(event, window, cx)
                    }
                } else {
                    handler(event, window, cx)
                }
            })
    }
}
