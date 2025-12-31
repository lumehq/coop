use gpui::http_client::Url;
use gpui::{
    div, px, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window,
};
use settings::AppSettings;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::switch::Switch;
use ui::{h_flex, v_flex, IconName, Sizable, Size, StyledExt};

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
}

impl Render for Preferences {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let auto_auth = AppSettings::get_auto_auth(cx);
        let backup = AppSettings::get_backup_messages(cx);
        let screening = AppSettings::get_screening(cx);
        let bypass = AppSettings::get_contact_bypass(cx);
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let hide = AppSettings::get_hide_user_avatars(cx);

        let input_state = self.media_input.downgrade();

        v_flex()
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
                            .child(SharedString::from("Relay and Media")),
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
                                                        Url::parse(&input.read(cx).value())
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
                                    .child(SharedString::from("Coop currently only supports NIP-96 media servers.")),
                            ),
                    )
                    .child(
                        Switch::new("auth")
                            .label("Automatically authenticate for known relays")
                            .description("After you approve the authentication request, Coop will automatically complete this step next time.")
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
                            .child(SharedString::from("Messages")),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                Switch::new("screening")
                                    .label("Screening")
                                    .description("When opening a chat request, Coop will show a popup to help you verify the sender.")
                                    .checked(screening)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_screening(!screening, cx);
                                    }),
                            )
                            .child(
                                Switch::new("bypass")
                                    .label("Skip screening for contacts")
                                    .description("Requests from your contacts will automatically go to inbox.")
                                    .checked(bypass)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_contact_bypass(!bypass, cx);
                                    }),
                            )
                            .child(
                                Switch::new("backup")
                                    .label("Backup messages")
                                    .description("When you send a message, Coop will also forward it to your configured Messaging Relays. Disabling this will cause all messages sent during the current session to disappear when the app is closed.")
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
                            .child(SharedString::from("Display")),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                Switch::new("hide_avatar")
                                    .label("Hide user avatars")
                                    .description("Unload all avatar pictures to improve performance and reduce memory usage.")
                                    .checked(hide)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_hide_user_avatars(!hide, cx);
                                    }),
                            )
                            .child(
                                Switch::new("proxy_avatar")
                                    .label("Proxy user avatars")
                                    .description("Use wsrv.nl to resize and downscale avatar pictures (saves ~50MB of data).")
                                    .checked(proxy)
                                    .on_click(move |_, _window, cx| {
                                        AppSettings::update_proxy_user_avatars(!proxy, cx);
                                    }),
                            ),
                    ),
            )
    }
}
