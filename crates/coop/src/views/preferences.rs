use common::display::ReadableProfile;
use gpui::http_client::Url;
use gpui::{
    div, px, relative, rems, App, AppContext, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
};
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use registry::Registry;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::switch::Switch;
use ui::{h_flex, v_flex, ContextModal, IconName, Sizable, Size, StyledExt};

use crate::views::{edit_profile, setup_relay};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Preferences> {
    cx.new(|cx| Preferences::new(window, cx))
}

pub struct Preferences {
    media_input: Entity<InputState>,
}

impl Preferences {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let media_server = AppSettings::get_media_server(cx).to_string();
        let media_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(media_server.clone())
                .placeholder(media_server)
        });

        Self { media_input }
    }

    fn open_edit_profile(&self, window: &mut Window, cx: &mut Context<Self>) {
        let view = edit_profile::init(window, cx);
        let weak_view = view.downgrade();
        let title = SharedString::new(t!("profile.title"));

        window.open_modal(cx, move |modal, _window, _cx| {
            let weak_view = weak_view.clone();

            modal
                .confirm()
                .title(title.clone())
                .child(view.clone())
                .button_props(ModalButtonProps::default().ok_text(t!("common.update")))
                .on_ok(move |_, window, cx| {
                    weak_view
                        .update(cx, |this, cx| {
                            let set_metadata = this.set_metadata(cx);

                            cx.spawn_in(window, async move |_, cx| {
                                match set_metadata.await {
                                    Ok(event) => {
                                        if let Some(event) = event {
                                            cx.update(|_, cx| {
                                                Registry::global(cx).update(cx, |this, cx| {
                                                    this.insert_or_update_person(event, cx);
                                                });
                                            })
                                            .ok();
                                        }
                                    }
                                    Err(e) => {
                                        cx.update(|window, cx| {
                                            window.push_notification(e.to_string(), cx);
                                        })
                                        .ok();
                                    }
                                };
                            })
                            .detach();
                        })
                        .ok();
                    // true to close the modal
                    true
                })
        });
    }

    fn open_relays(&self, window: &mut Window, cx: &mut Context<Self>) {
        let title = SharedString::new(t!("relays.modal_title"));
        let view = setup_relay::init(Kind::InboxRelays, window, cx);
        let weak_view = view.downgrade();

        window.open_modal(cx, move |this, _window, _cx| {
            let weak_view = weak_view.clone();

            this.confirm()
                .title(title.clone())
                .child(view.clone())
                .button_props(ModalButtonProps::default().ok_text(t!("common.update")))
                .on_ok(move |_, window, cx| {
                    weak_view
                        .update(cx, |this, cx| {
                            this.set_relays(window, cx);
                        })
                        .ok();
                    // true to close the modal
                    false
                })
        });
    }
}

impl Render for Preferences {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input_state = self.media_input.downgrade();
        let profile = Registry::read_global(cx).identity(cx);

        let auto_auth = AppSettings::get_auto_auth(cx);
        let backup = AppSettings::get_backup_messages(cx);
        let screening = AppSettings::get_screening(cx);
        let bypass = AppSettings::get_contact_bypass(cx);
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let hide = AppSettings::get_hide_user_avatars(cx);

        v_flex()
            .child(
                v_flex()
                    .pb_2()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child(shared_t!("preferences.account_header")),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(
                                h_flex()
                                    .id("user")
                                    .gap_2()
                                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(2.4)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_sm()
                                            .child(
                                                div()
                                                    .font_semibold()
                                                    .line_height(relative(1.3))
                                                    .child(profile.display_name()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().text_muted)
                                                    .line_height(relative(1.3))
                                                    .child(shared_t!("preferences.account_btn")),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.open_edit_profile(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("relays")
                                    .label("Messaging Relays")
                                    .xsmall()
                                    .ghost_alt()
                                    .rounded(ButtonRounded::Full)
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.open_relays(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child(shared_t!("preferences.relay_and_media")),
                    )
                    .child(
                        v_flex()
                            .my_1()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(TextInput::new(&self.media_input).xsmall())
                                    .child(
                                        Button::new("update")
                                            .icon(IconName::Check)
                                            .ghost()
                                            .with_size(Size::Size(px(26.)))
                                            .on_click(move |_, _window, cx| {
                                                if let Some(input) = input_state.upgrade() {
                                                    let Ok(url) =
                                                        Url::parse(input.read(cx).value())
                                                    else {
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
                                    .child(shared_t!("preferences.media_description")),
                            ),
                    )
                    .child(
                        Switch::new("auth")
                            .label(t!("preferences.auto_auth"))
                            .description(t!("preferences.auto_auth_description"))
                            .checked(auto_auth)
                            .on_click(move |_, _window, cx| {
                                AppSettings::update_auto_auth(!auto_auth, cx);
                            }),
                    ),
            )
            .child(
                v_flex()
                    .py_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child(shared_t!("preferences.messages_header")),
                    )
                    .child(
                        v_flex()
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
                                    .checked(bypass)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_contact_bypass(!bypass, cx);
                                    }),
                            )
                            .child(
                                Switch::new("backup")
                                    .label(t!("preferences.backup_label"))
                                    .description(t!("preferences.backup_description"))
                                    .checked(backup)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_backup_messages(!backup, cx);
                                    }),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .py_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child(shared_t!("preferences.display_header")),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                Switch::new("hide_avatar")
                                    .label(t!("preferences.hide_avatars_label"))
                                    .description(t!("preferences.hide_avatar_description"))
                                    .checked(hide)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_hide_user_avatars(!hide, cx);
                                    }),
                            )
                            .child(
                                Switch::new("proxy_avatar")
                                    .label(t!("preferences.proxy_avatars_label"))
                                    .description(t!("preferences.proxy_description"))
                                    .checked(proxy)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_proxy_user_avatars(!proxy, cx);
                                    }),
                            ),
                    ),
            )
    }
}
