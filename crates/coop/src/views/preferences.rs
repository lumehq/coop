use common::profile::RenderProfile;
use global::constants::{DEFAULT_MODAL_WIDTH, NIP96_SERVER};
use gpui::http_client::Url;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, App, AppContext, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};
use identity::Identity;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::switch::Switch;
use ui::{ContextModal, IconName, Sizable, Size, StyledExt};

use crate::views::{profile, relays};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Preferences> {
    Preferences::new(window, cx)
}

pub struct Preferences {
    media_input: Entity<InputState>,
    focus_handle: FocusHandle,
}

impl Preferences {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let media_server = AppSettings::get_global(cx)
                .settings
                .media_server
                .to_string();

            let media_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(media_server)
                    .placeholder(NIP96_SERVER)
            });

            Self {
                media_input,
                focus_handle: cx.focus_handle(),
            }
        })
    }

    fn open_profile(&self, window: &mut Window, cx: &mut Context<Self>) {
        let profile = profile::init(window, cx);

        window.open_modal(cx, move |modal, _, _| {
            modal
                .title("Profile")
                .width(px(DEFAULT_MODAL_WIDTH))
                .child(profile.clone())
        });
    }

    fn open_relays(&self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = relays::init(window, cx);

        window.open_modal(cx, move |this, _, _| {
            this.width(px(DEFAULT_MODAL_WIDTH))
                .title("Edit your Messaging Relays")
                .child(relays.clone())
        });
    }
}

impl Render for Preferences {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const MEDIA_DESCRIPTION: &str = "Coop currently only supports NIP-96 media servers. \
                                         If you're unsure, please keep the default value.";

        const BACKUP_DESCRIPTION: &str = "When you send a message, Coop will also send it to \
                                          your configured Messaging Relays. You can disable this \
                                          if you want all sent messages to disappear when you log out.";

        const HIDE_AVATAR_DESCRIPTION: &str = "Unload all avatar pictures to improve performance \
                                               and reduce memory usage.";

        const PROXY_DESCRIPTION: &str = "Use wsrv.nl to resize and downscale avatar pictures \
                                         (saves ~50MB of data).";

        let input_state = self.media_input.downgrade();
        let settings = AppSettings::get_global(cx).settings.as_ref();

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
                    .when_some(Identity::get_global(cx).profile(), |this, profile| {
                        this.child(
                            div()
                                .w_full()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .id("current-user")
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Avatar::new(
                                                profile.render_avatar(settings.proxy_user_avatars),
                                            )
                                            .size(rems(2.4)),
                                        )
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
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_profile(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("relays")
                                        .label("DM Relays")
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_relays(window, cx);
                                        })),
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
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_1()
                            .child(TextInput::new(&self.media_input).xsmall())
                            .child(
                                Button::new("update")
                                    .icon(IconName::CheckCircleFill)
                                    .ghost()
                                    .with_size(Size::Size(px(26.)))
                                    .on_click(move |_, window, cx| {
                                        if let Some(input) = input_state.upgrade() {
                                            let value = input.read(cx).value();
                                            let Ok(url) = Url::parse(value) else {
                                                window.push_notification("URL is not valid", cx);
                                                return;
                                            };

                                            AppSettings::global(cx).update(cx, |this, cx| {
                                                this.settings.media_server = url;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child(MEDIA_DESCRIPTION),
                    ),
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
                            .child("Messages"),
                    )
                    .child(
                        div().flex().flex_col().gap_2().child(
                            Switch::new("backup_messages")
                                .label("Backup messages")
                                .description(BACKUP_DESCRIPTION)
                                .checked(settings.backup_messages)
                                .on_click(|_, _window, cx| {
                                    AppSettings::global(cx).update(cx, |this, cx| {
                                        this.settings.backup_messages =
                                            !this.settings.backup_messages;
                                        cx.notify();
                                    })
                                }),
                        ),
                    ),
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
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                Switch::new("hide_user_avatars")
                                    .label("Hide user avatars")
                                    .description(HIDE_AVATAR_DESCRIPTION)
                                    .checked(settings.hide_user_avatars)
                                    .on_click(|_, _window, cx| {
                                        AppSettings::global(cx).update(cx, |this, cx| {
                                            this.settings.hide_user_avatars =
                                                !this.settings.hide_user_avatars;
                                            cx.notify();
                                        })
                                    }),
                            )
                            .child(
                                Switch::new("proxy_user_avatars")
                                    .label("Proxy user avatars")
                                    .description(PROXY_DESCRIPTION)
                                    .checked(settings.proxy_user_avatars)
                                    .on_click(|_, _window, cx| {
                                        AppSettings::global(cx).update(cx, |this, cx| {
                                            this.settings.proxy_user_avatars =
                                                !this.settings.proxy_user_avatars;
                                            cx.notify();
                                        })
                                    }),
                            ),
                    ),
            )
    }
}
