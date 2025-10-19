use std::time::Duration;

use anyhow::Error;
use common::display::RenderedProfile;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, svg, AnyElement, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    RetainAllImageCache, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    WeakEntity, Window,
};
use i18n::{shared_t, t};
use nostr_connect::prelude::*;
use registry::keystore::KeyItem;
use registry::Registry;
use smallvec::{smallvec, SmallVec};
use states::app_state;
use states::constants::{ACCOUNT_IDENTIFIER, BUNKER_TIMEOUT};
use states::state::SignalKind;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::indicator::Indicator;
use ui::input::{InputState, TextInput};
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
    secret: String,
    loading: bool,

    name: SharedString,
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,

    /// Background tasks
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Account {
    fn new(
        public_key: PublicKey,
        secret: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            public_key,
            secret,
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

        // Try to login with bunker
        if self.secret.starts_with("bunker://") {
            match NostrConnectURI::parse(&self.secret) {
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
        match EncryptedSecretKey::from_bech32(&self.secret) {
            Ok(enc) => {
                self.login_with_keys(enc, window, cx);
            }
            Err(e) => {
                window.push_notification(e.to_string(), cx);
                self.set_loading(false, cx);
            }
        }
    }

    fn login_with_bunker(
        &mut self,
        uri: NostrConnectURI,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystore = Registry::global(cx).read(cx).keystore();

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
                                let client = app_state().client();

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

    fn login_with_keys(
        &mut self,
        enc: EncryptedSecretKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

                    let client = app_state().client();
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
                let states = app_state();
                let client = states.client();

                let filter = Filter::new()
                    .kind(Kind::ApplicationSpecificData)
                    .identifier(ACCOUNT_IDENTIFIER);

                // Delete account
                client.database().delete(filter).await.ok();

                // Unset the client's signer
                client.unset_signer().await;

                // Notify the channel about the signer being unset
                states.signal().send(SignalKind::SignerUnset).await;
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
        let registry = Registry::global(cx);
        let profile = registry.read(cx).get_person(&self.public_key, cx);
        let bunker = self.secret.starts_with("bunker://");

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
