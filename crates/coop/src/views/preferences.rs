use common::display::DisplayProfile;
use global::constants::{DEFAULT_MODAL_WIDTH, NIP96_SERVER};
use gpui::http_client::Url;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, App, AppContext, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
};
use i18n::t;
use identity::Identity;
use registry::Registry;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::switch::Switch;
use ui::{ContextModal, IconName, Sizable, Size, StyledExt};

use crate::views::{edit_profile, relays};

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
            let media_server = AppSettings::get_media_server(cx).to_string();
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

    fn open_edit_profile(&self, window: &mut Window, cx: &mut Context<Self>) {
        let edit_profile = edit_profile::init(window, cx);
        let title = SharedString::new(t!("profile.title"));

        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .title(title.clone())
                .width(px(DEFAULT_MODAL_WIDTH))
                .child(edit_profile.clone())
        });
    }

    fn open_relays(&self, window: &mut Window, cx: &mut Context<Self>) {
        let relays = relays::init(window, cx);
        let title = SharedString::new(t!("preferences.modal_relays_title"));

        window.open_modal(cx, move |this, _window, _cx| {
            this.width(px(DEFAULT_MODAL_WIDTH))
                .title(title.clone())
                .child(relays.clone())
        });
    }
}

impl Render for Preferences {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input_state = self.media_input.downgrade();
        let profile = Identity::read_global(cx)
            .public_key()
            .map(|pk| Registry::read_global(cx).get_person(&pk, cx));

        let backup_messages = AppSettings::get_backup_messages(cx);
        let screening = AppSettings::get_screening(cx);
        let contact_bypass = AppSettings::get_contact_bypass(cx);
        let proxy_avatar = AppSettings::get_proxy_user_avatars(cx);
        let hide_avatar = AppSettings::get_hide_user_avatars(cx);

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
                            .child(SharedString::new(t!("preferences.account_header"))),
                    )
                    .when_some(profile, |this, profile| {
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
                                            Avatar::new(profile.avatar_url(proxy_avatar))
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
                                                        .child(profile.display_name()),
                                                )
                                                .child(
                                                    div()
                                                        .line_height(relative(1.3))
                                                        .text_xs()
                                                        .text_color(cx.theme().text_muted)
                                                        .child(SharedString::new(t!(
                                                            "preferences.see_your_profile"
                                                        ))),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _e, window, cx| {
                                            this.open_edit_profile(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("relays")
                                        .label("DM Relays")
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(move |this, _e, window, cx| {
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
                            .child(SharedString::new(t!("preferences.media_server_header"))),
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
                                            let Ok(url) = Url::parse(input.read(cx).value()) else {
                                                window.push_notification(
                                                    t!("preferences.url_not_valid"),
                                                    cx,
                                                );
                                                return;
                                            };
                                            AppSettings::update_media_server(url, cx);
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child(SharedString::new(t!("preferences.media_description"))),
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
                            .child(SharedString::new(t!("preferences.messages_header"))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                Switch::new("screening")
                                    .label(t!("preferences.screening_label"))
                                    .description(t!("preferences.screening_description"))
                                    .checked(screening)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_screening(!screening, cx);
                                    }),
                            )
                            .child(
                                Switch::new("bypass")
                                    .label(t!("preferences.bypass_label"))
                                    .description(t!("preferences.bypass_description"))
                                    .checked(contact_bypass)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_contact_bypass(!contact_bypass, cx);
                                    }),
                            )
                            .child(
                                Switch::new("backup_messages")
                                    .label(t!("preferences.backup_messages_label"))
                                    .description(t!("preferences.backup_description"))
                                    .checked(backup_messages)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_backup_messages(!backup_messages, cx);
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
                            .child(SharedString::new(t!("preferences.display_header"))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                Switch::new("hide_user_avatars")
                                    .label(t!("preferences.hide_avatars_label"))
                                    .description(t!("preferences.hide_avatar_description"))
                                    .checked(hide_avatar)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_hide_user_avatars(!hide_avatar, cx);
                                    }),
                            )
                            .child(
                                Switch::new("proxy_user_avatars")
                                    .label(t!("preferences.proxy_avatars_label"))
                                    .description(t!("preferences.proxy_description"))
                                    .checked(proxy_avatar)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_proxy_user_avatars(!proxy_avatar, cx);
                                    }),
                            ),
                    ),
            )
    }
}
