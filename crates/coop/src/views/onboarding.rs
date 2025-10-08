use std::sync::Arc;
use std::time::Duration;

use client_keys::ClientKeys;
use common::display::TextUtils;
use global::constants::{APP_NAME, NOSTR_CONNECT_RELAY, NOSTR_CONNECT_TIMEOUT};
use global::identiers::account_identifier;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, px, relative, svg, AnyElement, App, AppContext, ClipboardItem, Context, Entity,
    EventEmitter, FocusHandle, Focusable, Image, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window,
};
use i18n::{shared_t, t};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::{divider, h_flex, v_flex, ContextModal, Icon, IconName, Sizable, StyledExt};

use crate::chatspace::{self, ChatSpace};

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
    nostr_connect_uri: Entity<NostrConnectURI>,
    nostr_connect: Entity<Option<NostrConnect>>,
    qr_code: Entity<Option<Arc<Image>>>,
    connecting: bool,
    // Panel
    name: SharedString,
    focus_handle: FocusHandle,
    _subscriptions: SmallVec<[Subscription; 2]>,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Onboarding {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let nostr_connect = cx.new(|_| None);
        let qr_code = cx.new(|_| None);

        // NIP46: https://github.com/nostr-protocol/nips/blob/master/46.md
        //
        // Direct connection initiated by the client
        let nostr_connect_uri = cx.new(|cx| {
            let relay = RelayUrl::parse(NOSTR_CONNECT_RELAY).unwrap();
            let app_keys = ClientKeys::read_global(cx).keys();
            NostrConnectURI::client(app_keys.public_key(), vec![relay], APP_NAME)
        });

        let mut subscriptions = smallvec![];

        // Clean up when the current view is released
        subscriptions.push(cx.on_release_in(window, |this, window, cx| {
            this.shutdown_nostr_connect(window, cx);
        }));

        // Set Nostr Connect after the view is initialized
        cx.defer_in(window, |this, window, cx| {
            this.set_connect(window, cx);
        });

        Self {
            nostr_connect,
            nostr_connect_uri,
            qr_code,
            connecting: false,
            name: "Onboarding".into(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
            _tasks: smallvec![],
        }
    }

    fn set_connecting(&mut self, cx: &mut Context<Self>) {
        self.connecting = true;
        cx.notify();
    }

    fn set_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let uri = self.nostr_connect_uri.read(cx).clone();
        let app_keys = ClientKeys::read_global(cx).keys();
        let timeout = Duration::from_secs(NOSTR_CONNECT_TIMEOUT);

        self.qr_code.update(cx, |this, cx| {
            *this = uri.to_string().to_qr();
            cx.notify();
        });

        self.nostr_connect.update(cx, |this, cx| {
            *this = NostrConnect::new(uri, app_keys, timeout, None).ok();
            cx.notify();
        });

        self._tasks.push(
            // Wait for Nostr Connect approval
            cx.spawn_in(window, async move |this, cx| {
                let connect = this.read_with(cx, |this, cx| this.nostr_connect.read(cx).clone());

                if let Ok(Some(signer)) = connect {
                    match signer.bunker_uri().await {
                        Ok(uri) => {
                            this.update(cx, |this, cx| {
                                this.set_connecting(cx);
                                this.write_uri_to_disk(signer, uri, cx);
                            })
                            .ok();
                        }
                        Err(e) => {
                            this.update_in(cx, |_, window, cx| {
                                window.push_notification(
                                    Notification::error(e.to_string()).title("Nostr Connect"),
                                    cx,
                                );
                            })
                            .ok();
                        }
                    };
                }
            }),
        )
    }

    fn set_proxy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        ChatSpace::proxy_signer(window, cx);
    }

    fn write_uri_to_disk(
        &mut self,
        signer: NostrConnect,
        uri: NostrConnectURI,
        cx: &mut Context<Self>,
    ) {
        let mut uri_without_secret = uri.to_string();

        // Clear the secret parameter in the URI if it exists
        if let Some(secret) = uri.secret() {
            uri_without_secret = uri_without_secret.replace(secret, "");
        }

        let task: Task<Result<(), anyhow::Error>> = cx.background_spawn(async move {
            let client = nostr_client();

            // Update the client's signer
            client.set_signer(signer).await;

            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;

            let event = EventBuilder::new(Kind::ApplicationSpecificData, uri_without_secret)
                .tag(account_identifier().to_owned())
                .build(public_key)
                .sign(&Keys::generate())
                .await?;

            // Save the event to the database
            client.database().save_event(&event).await?;

            Ok(())
        });

        task.detach();
    }

    fn copy_uri(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.nostr_connect_uri.read(cx).to_string(),
        ));
        window.push_notification(t!("common.copied"), cx);
    }

    fn shutdown_nostr_connect(&mut self, _window: &mut Window, cx: &mut App) {
        if !self.connecting {
            if let Some(signer) = self.nostr_connect.read(cx).clone() {
                cx.background_spawn(async move {
                    log::info!("Shutting down Nostr Connect");
                    signer.shutdown().await;
                })
                .detach();
            }
        }
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
            .bg(cx.theme().ghost_element_background_alt)
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
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }
}

impl EventEmitter<PanelEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Onboarding {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                                    .text_color(cx.theme().elevated_surface_background),
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
                                            .text_color(cx.theme().text_muted)
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
                                    .bold()
                                    .reverse()
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        chatspace::new_account(window, cx);
                                    })),
                            )
                            .child(
                                h_flex()
                                    .my_1()
                                    .gap_1()
                                    .child(divider(cx))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().text_muted)
                                            .child(shared_t!("onboarding.divider")),
                                    )
                                    .child(divider(cx)),
                            )
                            .child(
                                Button::new("key")
                                    .label(t!("onboarding.key_login"))
                                    .ghost_alt()
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        chatspace::login(window, cx);
                                    })),
                            )
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        Button::new("ext")
                                            .label(t!("onboarding.ext_login"))
                                            .ghost_alt()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.set_proxy(window, cx);
                                            })),
                                    )
                                    .child(
                                        div()
                                            .italic()
                                            .text_xs()
                                            .text_center()
                                            .text_color(cx.theme().text_muted)
                                            .child(shared_t!("onboarding.ext_login_note")),
                                    ),
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
                            .bg(cx.theme().surface_background)
                            .rounded_2xl()
                            .child(
                                v_flex()
                                    .gap_5()
                                    .items_center()
                                    .justify_center()
                                    .when_some(self.qr_code.read(cx).as_ref(), |this, qr| {
                                        this.child(
                                            div()
                                                .id("")
                                                .child(
                                                    img(qr.clone())
                                                        .size(px(256.))
                                                        .rounded_xl()
                                                        .shadow_lg()
                                                        .border_1()
                                                        .border_color(cx.theme().element_active),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _e, window, cx| {
                                                        this.copy_uri(window, cx)
                                                    },
                                                )),
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
                                                    .text_color(cx.theme().text_muted)
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
