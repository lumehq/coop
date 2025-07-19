use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, rems, App, ClickEvent, Div, InteractiveElement, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
use i18n::t;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::actions::OpenProfile;
use ui::avatar::Avatar;
use ui::context_menu::ContextMenuExt;
use ui::StyledExt;

#[derive(IntoElement)]
pub struct RoomListItem {
    ix: usize,
    base: Div,
    public_key: PublicKey,
    img: Option<SharedString>,
    label: Option<SharedString>,
    description: Option<SharedString>,
    #[allow(clippy::type_complexity)]
    handler: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl RoomListItem {
    pub fn new(ix: usize, public_key: PublicKey) -> Self {
        Self {
            ix,
            base: div().h_9().w_full().px_1p5(),
            public_key,
            img: None,
            label: None,
            description: None,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn img(mut self, img: impl Into<SharedString>) -> Self {
        self.img = Some(img.into());
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
        let handler = self.handler.clone();
        let hide_avatar = AppSettings::get_global(cx).settings.hide_user_avatars;

        self.base
            .id(self.ix)
            .flex()
            .items_center()
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
                            if let Some(path) = self.img {
                                this.child(Avatar::new(path).size(rems(1.5)))
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
                    .when_some(self.label, |this, label| {
                        this.child(
                            div()
                                .flex_1()
                                .line_clamp(1)
                                .text_ellipsis()
                                .font_medium()
                                .child(label),
                        )
                    })
                    .when_some(self.description, |this, description| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().text_placeholder)
                                .child(description),
                        )
                    }),
            )
            .hover(|this| this.bg(cx.theme().elevated_surface_background))
            .on_click(move |ev, window, cx| handler(ev, window, cx))
            .context_menu(move |this, _window, _cx| {
                // TODO: add share chat room
                this.menu(t!("profile.view"), Box::new(OpenProfile(public_key)))
            })
    }
}
