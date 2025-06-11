use common::profile::RenderProfile;
use global::shared_state;
use gpui::{
    div, prelude::FluentBuilder, relative, rems, App, AppContext, Context, Entity, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Window,
};
use settings::Settings;
use theme::ActiveTheme;
use ui::{avatar::Avatar, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Preferences> {
    Preferences::new(window, cx)
}

pub struct Preferences {
    focus_handle: FocusHandle,
}

impl Preferences {
    pub fn new(_window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl Render for Preferences {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = Settings::get_global(cx).proxy_user_avatars;

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .px_3()
            .pb_3()
            .flex()
            .flex_col()
            .child(
                div()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child("Account"),
                    )
                    .when_some(shared_state().identity(), |this, profile| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(Avatar::new(profile.render_avatar(proxy)).size(rems(2.4)))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .child(
                                            div()
                                                .line_height(relative(1.3))
                                                .font_semibold()
                                                .child(profile.render_name()),
                                        )
                                        .child(
                                            div()
                                                .line_height(relative(1.3))
                                                .text_xs()
                                                .text_color(cx.theme().text_muted)
                                                .child("See your profile"),
                                        ),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child("Media Server"),
                    )
                    .child(div().child("TODO")),
            )
            .child(
                div()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child("Display"),
                    )
                    .child(div().child("TODO")),
            )
    }
}
