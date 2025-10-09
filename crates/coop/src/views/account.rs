use std::time::Duration;

use anyhow::Error;
use common::display::RenderedProfile;
use global::app_state::{AppIdentifierTag, SignalKind};
use global::constants::{ACCOUNT_PATH, BUNKER_TIMEOUT};
use global::{app_state, nostr_client};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    RetainAllImageCache, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    WeakEntity, Window,
};
use i18n::{shared_t, t};
use nostr_connect::prelude::*;
use registry::Registry;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::input::{InputState, TextInput};
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::{h_flex, v_flex, ContextModal, Sizable, StyledExt};

use crate::actions::CoopAuthUrlHandler;

pub fn init(
    public_key: PublicKey,
    secret: String,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Account> {
    cx.new(|cx| Account::new(public_key, secret, window, cx))
}

pub struct Account {
    public_key: PublicKey,
    stored_secret: String,
    bunker_keys: Entity<Option<Keys>>,
    loading: bool,

    name: SharedString,
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,

    _subscriptions: SmallVec<[Subscription; 1]>,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Account {
    fn new(
        public_key: PublicKey,
        secret: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let bunker_keys = cx.new(|_| None);

        let mut tasks = smallvec![];
        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Clear the image cache when user closes the account panel
            cx.on_release_in(window, move |this, window, cx| {
                this.image_cache.update(cx, |this, cx| {
                    this.clear(window, cx);
                });
            }),
        );

        if secret.starts_with("bunker://") {
            tasks.push(
                // Load the bunker keys for re-connection
                cx.spawn(async move |this, cx| {
                    let app_state = app_state();

                    if let Ok(content) = app_state.load_from_db(AppIdentifierTag::Bunker).await {
                        if let Ok(secret) = SecretKey::parse(&content) {
                            let keys = Keys::new(secret);

                            this.update(cx, |this, cx| {
                                this.bunker_keys.update(cx, |this, cx| {
                                    *this = Some(keys);
                                    cx.notify();
                                });
                            })
                            .ok();
                        }
                    }
                }),
            );
        }

        Self {
            public_key,
            bunker_keys,
            stored_secret: secret,
            loading: false,
            name: "Account".into(),
            focus_handle: cx.focus_handle(),
            image_cache: RetainAllImageCache::new(cx),
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_loading(true, cx);

        if self.stored_secret.starts_with("bunker://") {
            if let Ok(uri) = NostrConnectURI::parse(&self.stored_secret) {
                self.nostr_connect(uri, window, cx);
            }
        } else if let Ok(enc) = EncryptedSecretKey::from_bech32(&self.stored_secret) {
            self.keys(enc, window, cx);
        } else {
            window.push_notification("Cannot continue with current account", cx);
            self.set_loading(false, cx);
        }
    }

    fn nostr_connect(&mut self, uri: NostrConnectURI, window: &mut Window, cx: &mut Context<Self>) {
        let Some(keys) = self.bunker_keys.read(cx).clone() else {
            return;
        };

        let timeout = Duration::from_secs(BUNKER_TIMEOUT);
        let mut signer = NostrConnect::new(uri, keys, timeout, None).unwrap();

        // Handle auth url with the default browser
        signer.auth_url_handler(CoopAuthUrlHandler);

        self._tasks.push(
            // Handle connection in the background
            cx.spawn_in(window, async move |this, cx| {
                match signer.bunker_uri().await {
                    Ok(_) => {
                        let client = nostr_client();
                        // Set the client's signer with the current nostr connect instance
                        client.set_signer(signer).await;
                    }
                    Err(e) => {
                        this.update_in(cx, |this, window, cx| {
                            this.set_loading(false, cx);
                            window.push_notification(Notification::error(e.to_string()), cx);
                        })
                        .ok();
                    }
                }
            }),
        );
    }

    fn keys(&mut self, enc: EncryptedSecretKey, window: &mut Window, cx: &mut Context<Self>) {
        let pwd_input: Entity<InputState> = cx.new(|cx| InputState::new(window, cx).masked(true));
        let weak_input = pwd_input.downgrade();

        let error: Entity<Option<SharedString>> = cx.new(|_| None);
        let weak_error = error.downgrade();

        let entity = cx.weak_entity();

        window.open_modal(cx, move |this, _window, cx| {
            let entity = entity.clone();
            let entity_clone = entity.clone();
            let weak_input = weak_input.clone();
            let weak_error = weak_error.clone();

            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .on_cancel(move |_, _window, cx| {
                    entity
                        .update(cx, |this, cx| {
                            this.set_loading(false, cx);
                        })
                        .ok();

                    // true to close the modal
                    true
                })
                .on_ok(move |_, window, cx| {
                    let weak_error = weak_error.clone();
                    let password = weak_input
                        .read_with(cx, |state, _cx| state.value().to_owned())
                        .ok();

                    entity_clone
                        .update(cx, |this, cx| {
                            this.verify_keys(enc, password, weak_error, window, cx);
                        })
                        .ok();

                    // false to keep the modal open
                    false
                })
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .child(shared_t!("login.password_to_decrypt"))
                        .child(TextInput::new(&pwd_input).small())
                        .when_some(error.read(cx).as_ref(), |this, error| {
                            this.child(
                                div()
                                    .text_xs()
                                    .italic()
                                    .text_color(cx.theme().danger_foreground)
                                    .child(error.clone()),
                            )
                        }),
                )
        });
    }

    fn verify_keys(
        &mut self,
        enc: EncryptedSecretKey,
        password: Option<SharedString>,
        error: WeakEntity<Option<SharedString>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(password) = password else {
            error
                .update(cx, |this, cx| {
                    *this = Some("Password is required".into());
                    cx.notify();
                })
                .ok();
            return;
        };

        if password.is_empty() {
            error
                .update(cx, |this, cx| {
                    *this = Some("Password cannot be empty".into());
                    cx.notify();
                })
                .ok();
            return;
        }

        let task: Task<Result<SecretKey, Error>> = cx.background_spawn(async move {
            let secret = enc.decrypt(&password)?;
            Ok(secret)
        });

        cx.spawn_in(window, async move |_this, cx| {
            match task.await {
                Ok(secret) => {
                    cx.update(|window, cx| {
                        window.close_all_modals(cx);
                    })
                    .ok();

                    let client = nostr_client();
                    let keys = Keys::new(secret);

                    // Set the client's signer with the current keys
                    client.set_signer(keys).await
                }
                Err(e) => {
                    error
                        .update(cx, |this, cx| {
                            *this = Some(e.to_string().into());
                            cx.notify();
                        })
                        .ok();
                }
            };
        })
        .detach();
    }

    fn logout(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self._tasks.push(
            // Reset the nostr client in the background
            cx.background_spawn(async move {
                let client = nostr_client();
                let app_state = app_state();

                let filter = Filter::new()
                    .kind(Kind::ApplicationSpecificData)
                    .identifier(ACCOUNT_PATH);

                // Delete account
                client.database().delete(filter).await.ok();

                // Unset the client's signer
                client.unset_signer().await;

                // Notify the channel about the signer being unset
                app_state.signal.send(SignalKind::SignerUnset).await;
            }),
        );
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.loading = status;
        cx.notify();
    }
}

impl Panel for Account {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }
}

impl EventEmitter<PanelEvent> for Account {}

impl Focusable for Account {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Account {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let registry = Registry::read_global(cx);
        let profile = registry.get_person(&self.public_key, cx);
        let is_bunker = self.stored_secret.starts_with("bunker://");

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
                                        .when(is_bunker, |this| {
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
                                        }),
                                )
                            })
                            .active(|this| this.bg(cx.theme().element_active))
                            .hover(|this| this.bg(cx.theme().element_hover))
                            .on_click(cx.listener(move |this, _e, window, cx| {
                                this.login(window, cx);
                            })),
                    )
                    .child(
                        Button::new("logout")
                            .label(t!("user.sign_out"))
                            .ghost()
                            .on_click(cx.listener(move |this, _e, window, cx| {
                                this.logout(window, cx);
                            })),
                    ),
            )
    }
}
