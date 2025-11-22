use std::time::Duration;

use common::{RenderedProfile, BUNKER_TIMEOUT};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    RetainAllImageCache, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    Window,
};
use i18n::{shared_t, t};
use key_store::{Credential, KeyItem, KeyStore};
use nostr_connect::prelude::*;
use person::PersonRegistry;
use smallvec::{smallvec, SmallVec};
use state::NostrRegistry;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::{h_flex, v_flex, ContextModal, Sizable, StyledExt};

use crate::actions::{reset, CoopAuthUrlHandler};

pub fn init(cre: Credential, window: &mut Window, cx: &mut App) -> Entity<Startup> {
    cx.new(|cx| Startup::new(cre, window, cx))
}

/// Startup
#[derive(Debug)]
pub struct Startup {
    credential: Credential,
    loading: bool,

    name: SharedString,
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Startup {
    fn new(credential: Credential, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tasks = smallvec![];
        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Clear the local state when user closes the account panel
            cx.on_release_in(window, move |this, window, cx| {
                this.image_cache.update(cx, |this, cx| {
                    this.clear(window, cx);
                });
            }),
        );

        Self {
            credential,
            loading: false,
            name: "Onboarding".into(),
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_loading(true, cx);

        let secret = self.credential.secret();

        // Try to login with bunker
        if secret.starts_with("bunker://") {
            match NostrConnectUri::parse(secret) {
                Ok(uri) => {
                    self.login_with_bunker(uri, window, cx);
                }
                Err(e) => {
                    window.push_notification(e.to_string(), cx);
                    self.set_loading(false, cx);
                }
            }
            return;
        };

        // Fall back to login with keys
        match SecretKey::parse(secret) {
            Ok(secret) => {
                self.login_with_keys(secret, cx);
            }
            Err(e) => {
                window.push_notification(e.to_string(), cx);
                self.set_loading(false, cx);
            }
        }
    }

    fn login_with_bunker(
        &mut self,
        uri: NostrConnectUri,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let keystore = KeyStore::global(cx).read(cx).backend();

        // Handle connection in the background
        cx.spawn_in(window, async move |this, cx| {
            let result = keystore
                .read_credentials(&KeyItem::Bunker.to_string(), cx)
                .await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Some((_, content))) => {
                        let secret = SecretKey::from_slice(&content).unwrap();
                        let keys = Keys::new(secret);
                        let timeout = Duration::from_secs(BUNKER_TIMEOUT);
                        let mut signer = NostrConnect::new(uri, keys, timeout, None).unwrap();

                        // Handle auth url with the default browser
                        signer.auth_url_handler(CoopAuthUrlHandler);

                        // Connect to the remote signer
                        this._tasks.push(
                            // Handle connection in the background
                            cx.spawn_in(window, async move |this, cx| {
                                match signer.bunker_uri().await {
                                    Ok(_) => {
                                        client.set_signer(signer).await;
                                    }
                                    Err(e) => {
                                        this.update_in(cx, |this, window, cx| {
                                            window.push_notification(e.to_string(), cx);
                                            this.set_loading(false, cx);
                                        })
                                        .ok();
                                    }
                                }
                            }),
                        )
                    }
                    Ok(None) => {
                        window.push_notification(t!("login.keyring_required"), cx);
                        this.set_loading(false, cx);
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                        this.set_loading(false, cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn login_with_keys(&mut self, secret: SecretKey, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();
        let keys = Keys::new(secret);

        // Update the signer
        cx.background_spawn(async move {
            client.set_signer(keys).await;
        })
        .detach();
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.loading = status;
        cx.notify();
    }
}

impl Panel for Startup {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }
}

impl EventEmitter<PanelEvent> for Startup {}

impl Focusable for Startup {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Startup {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let persons = PersonRegistry::global(cx);
        let bunker = self.credential.secret().starts_with("bunker://");
        let profile = persons
            .read(cx)
            .get_person(&self.credential.public_key(), cx);

        v_flex()
            .image_cache(self.image_cache.clone())
            .relative()
            .size_full()
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
                    .gap_2()
                    .child(
                        div()
                            .id("account")
                            .h_10()
                            .w_72()
                            .bg(cx.theme().elevated_surface_background)
                            .rounded_lg()
                            .text_sm()
                            .when(self.loading, |this| {
                                this.child(
                                    div()
                                        .size_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(Indicator::new().small()),
                                )
                            })
                            .when(!self.loading, |this| {
                                let avatar = profile.avatar(true);
                                let name = profile.display_name();

                                this.child(
                                    h_flex()
                                        .h_full()
                                        .justify_center()
                                        .gap_2()
                                        .child(
                                            h_flex()
                                                .gap_1()
                                                .child(Avatar::new(avatar).size(rems(1.5)))
                                                .child(div().pb_px().font_semibold().child(name)),
                                        )
                                        .child(div().when(bunker, |this| {
                                            let label = SharedString::from("Nostr Connect");

                                            this.child(
                                                div()
                                                    .py_0p5()
                                                    .px_2()
                                                    .text_xs()
                                                    .bg(cx.theme().secondary_active)
                                                    .text_color(cx.theme().secondary_foreground)
                                                    .rounded_full()
                                                    .child(label),
                                            )
                                        })),
                                )
                            })
                            .text_color(cx.theme().text)
                            .active(|this| {
                                this.text_color(cx.theme().element_foreground)
                                    .bg(cx.theme().element_active)
                            })
                            .hover(|this| {
                                this.text_color(cx.theme().element_foreground)
                                    .bg(cx.theme().element_hover)
                            })
                            .on_click(cx.listener(move |this, _e, window, cx| {
                                this.login(window, cx);
                            })),
                    )
                    .child(
                        Button::new("logout")
                            .label(t!("user.sign_out"))
                            .ghost()
                            .on_click(|_, _window, cx| {
                                reset(cx);
                            }),
                    ),
            )
    }
}
