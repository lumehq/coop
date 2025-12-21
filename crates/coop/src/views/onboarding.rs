use std::sync::Arc;
use std::time::Duration;

use common::{TextUtils, CLIENT_NAME, NOSTR_CONNECT_RELAY, NOSTR_CONNECT_TIMEOUT};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, px, relative, svg, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Image, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Task, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::divider::Divider;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::notification::Notification;
use gpui_component::{h_flex, v_flex, ActiveTheme, Icon, IconName, Sizable, StyledExt, WindowExt};
use i18n::{shared_t, t};
use key_store::{KeyItem, KeyStore};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;

use crate::chatspace::{self};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Onboarding> {
    Onboarding::new(window, cx)
}

#[derive(Debug, Clone)]
pub enum NostrConnectApp {
    Nsec(String),
    Amber(String),
    Aegis(String),
}

impl NostrConnectApp {
    pub fn all() -> Vec<Self> {
        vec![
            NostrConnectApp::Nsec("https://nsec.app".to_string()),
            NostrConnectApp::Amber("https://github.com/greenart7c3/Amber".to_string()),
            NostrConnectApp::Aegis("https://github.com/ZharlieW/Aegis".to_string()),
        ]
    }

    pub fn url(&self) -> &str {
        match self {
            Self::Nsec(url) | Self::Amber(url) | Self::Aegis(url) => url,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            NostrConnectApp::Nsec(_) => "nsec.app (Desktop)".into(),
            NostrConnectApp::Amber(_) => "Amber (Android)".into(),
            NostrConnectApp::Aegis(_) => "Aegis (iOS)".into(),
        }
    }
}

pub struct Onboarding {
    app_keys: Keys,
    qr_code: Option<Arc<Image>>,

    /// Panel
    name: SharedString,
    focus_handle: FocusHandle,

    /// Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_keys = Keys::generate();
        let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT);

        let relay = RelayUrl::parse(NOSTR_CONNECT_RELAY).unwrap();
        let uri = NostrConnectUri::client(app_keys.public_key(), vec![relay], CLIENT_NAME);
        let qr_code = uri.to_string().to_qr();

        // NIP46: https://github.com/nostr-protocol/nips/blob/master/46.md
        //
        // Direct connection initiated by the client
        let signer = NostrConnect::new(uri, app_keys.clone(), timeout, None).unwrap();

        let mut tasks = smallvec![];

        tasks.push(
            // Wait for nostr connect
            cx.spawn_in(window, async move |this, cx| {
                let result = signer.bunker_uri().await;

                this.update_in(cx, |this, window, cx| {
                    match result {
                        Ok(uri) => {
                            this.save_connection(&uri, window, cx);
                            this.connect(signer, cx);
                        }
                        Err(e) => {
                            window.push_notification(Notification::error(e.to_string()), cx);
                        }
                    };
                })
                .ok();
            }),
        );

        Self {
            qr_code,
            app_keys,
            name: "Onboarding".into(),
            focus_handle: cx.focus_handle(),
            _tasks: tasks,
        }
    }

    fn save_connection(
        &mut self,
        uri: &NostrConnectUri,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystore = KeyStore::global(cx).read(cx).backend();
        let username = self.app_keys.public_key().to_hex();
        let secret = self.app_keys.secret_key().to_secret_bytes();
        let mut clean_uri = uri.to_string();

        // Clear the secret parameter in the URI if it exists
        if let Some(s) = uri.secret() {
            clean_uri = clean_uri.replace(s, "");
        }

        cx.spawn_in(window, async move |this, cx| {
            let user_url = KeyItem::User.to_string();
            let bunker_url = KeyItem::Bunker.to_string();
            let user_password = clean_uri.into_bytes();

            // Write bunker uri to keyring for further connection
            if let Err(e) = keystore
                .write_credentials(&user_url, "bunker", &user_password, cx)
                .await
            {
                this.update_in(cx, |_, window, cx| {
                    window.push_notification(e.to_string(), cx);
                })
                .ok();
            }

            // Write the app keys for further connection
            if let Err(e) = keystore
                .write_credentials(&bunker_url, &username, &secret, cx)
                .await
            {
                this.update_in(cx, |_, window, cx| {
                    window.push_notification(e.to_string(), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn connect(&mut self, signer: NostrConnect, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        cx.background_spawn(async move {
            client.set_signer(signer).await;
        })
        .detach();
    }

    fn render_apps(&self, cx: &Context<Self>) -> impl IntoIterator<Item = impl IntoElement> {
        let all_apps = NostrConnectApp::all();
        let mut items = Vec::with_capacity(all_apps.len());

        for (ix, item) in all_apps.into_iter().enumerate() {
            items.push(self.render_app(ix, item.as_str(), item.url(), cx));
        }

        items
    }

    fn render_app<T>(&self, ix: usize, label: T, url: &str, cx: &Context<Self>) -> impl IntoElement
    where
        T: Into<SharedString>,
    {
        div()
            .id(ix)
            .flex_1()
            .rounded_md()
            .py_0p5()
            .px_2()
            .bg(cx.theme().list)
            .child(label.into())
            .on_click({
                let url = url.to_owned();
                move |_e, _window, cx| {
                    cx.open_url(&url);
                }
            })
    }
}

impl Panel for Onboarding {
    fn panel_name(&self) -> &'static str {
        "Onboarding"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Onboarding")
    }
}

impl EventEmitter<PanelEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .gap_10()
                    .items_center()
                    .justify_center()
                    .child(
                        v_flex()
                            .items_center()
                            .justify_center()
                            .gap_4()
                            .child(
                                svg()
                                    .path("brand/coop.svg")
                                    .size_16()
                                    .text_color(cx.theme().muted),
                            )
                            .child(
                                div()
                                    .text_center()
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_semibold()
                                            .line_height(relative(1.3))
                                            .child(shared_t!("welcome.title")),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(shared_t!("welcome.subtitle")),
                                    ),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_80()
                            .gap_3()
                            .child(
                                Button::new("continue_btn")
                                    .icon(Icon::new(IconName::ArrowRight))
                                    .label(shared_t!("onboarding.start_messaging"))
                                    .primary()
                                    .large()
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        chatspace::new_account(window, cx);
                                    })),
                            )
                            .child(
                                h_flex()
                                    .my_1()
                                    .gap_1()
                                    .child(Divider::horizontal())
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(shared_t!("onboarding.divider")),
                                    )
                                    .child(Divider::horizontal()),
                            )
                            .child(
                                Button::new("key")
                                    .label(t!("onboarding.key_login"))
                                    .large()
                                    .link()
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        chatspace::login(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .p_2()
                    .flex_1()
                    .h_full()
                    .rounded_2xl()
                    .child(
                        v_flex()
                            .size_full()
                            .justify_center()
                            .bg(cx.theme().muted)
                            .rounded_2xl()
                            .child(
                                v_flex()
                                    .gap_5()
                                    .items_center()
                                    .justify_center()
                                    .when_some(self.qr_code.as_ref(), |this, qr| {
                                        this.child(
                                            img(qr.clone())
                                                .size(px(256.))
                                                .rounded_xl()
                                                .shadow_lg()
                                                .border_1()
                                                .border_color(cx.theme().primary_active),
                                        )
                                    })
                                    .child(
                                        v_flex()
                                            .justify_center()
                                            .items_center()
                                            .text_center()
                                            .child(
                                                div()
                                                    .font_semibold()
                                                    .line_height(relative(1.3))
                                                    .child(shared_t!("onboarding.nostr_connect")),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(shared_t!("onboarding.scan_qr")),
                                            )
                                            .child(
                                                h_flex()
                                                    .mt_2()
                                                    .gap_1()
                                                    .text_xs()
                                                    .justify_center()
                                                    .children(self.render_apps(cx)),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}
