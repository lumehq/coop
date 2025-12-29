use std::time::Duration;

use anyhow::anyhow;
use common::BUNKER_TIMEOUT;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use key_store::{KeyItem, KeyStore};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use state::client;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::{v_flex, ContextModal, Disableable, StyledExt};

use crate::actions::CoopAuthUrlHandler;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Login> {
    cx.new(|cx| Login::new(window, cx))
}

#[derive(Debug)]
pub struct Login {
    key_input: Entity<InputState>,
    pass_input: Entity<InputState>,
    error: Entity<Option<SharedString>>,
    countdown: Entity<Option<u64>>,
    require_password: bool,
    logging_in: bool,

    /// Panel
    name: SharedString,
    focus_handle: FocusHandle,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,
}

impl Login {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let key_input = cx.new(|cx| InputState::new(window, cx));
        let pass_input = cx.new(|cx| InputState::new(window, cx).masked(true));

        let error = cx.new(|_| None);
        let countdown = cx.new(|_| None);

        let mut subscriptions = smallvec![];

        subscriptions.push(
            // Subscribe to key input events and process login when the user presses enter
            cx.subscribe_in(&key_input, window, |this, input, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.login(window, cx);
                    }
                    InputEvent::Change => {
                        if input.read(cx).value().starts_with("ncryptsec1") {
                            this.require_password = true;
                            cx.notify();
                        }
                    }
                    _ => {}
                };
            }),
        );

        Self {
            key_input,
            pass_input,
            error,
            countdown,
            name: "Welcome Back".into(),
            focus_handle: cx.focus_handle(),
            logging_in: false,
            require_password: false,
            _subscriptions: subscriptions,
        }
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.logging_in {
            return;
        };

        // Prevent duplicate login requests
        self.set_logging_in(true, cx);

        let value = self.key_input.read(cx).value();
        let password = self.pass_input.read(cx).value();

        if value.starts_with("bunker://") {
            self.login_with_bunker(&value, window, cx);
        } else if value.starts_with("ncryptsec1") {
            self.login_with_password(&value, &password, cx);
        } else if value.starts_with("nsec1") {
            if let Ok(secret) = SecretKey::parse(&value) {
                let keys = Keys::new(secret);
                self.login_with_keys(keys, cx);
            } else {
                self.set_error("Invalid", cx);
            }
        } else {
            self.set_error("Invalid", cx);
        }
    }

    fn login_with_bunker(&mut self, content: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(uri) = NostrConnectUri::parse(content) else {
            self.set_error("Bunker is not valid", cx);
            return;
        };

        let app_keys = Keys::generate();
        let timeout = Duration::from_secs(BUNKER_TIMEOUT);
        let mut signer = NostrConnect::new(uri, app_keys.clone(), timeout, None).unwrap();

        // Handle auth url with the default browser
        signer.auth_url_handler(CoopAuthUrlHandler);

        // Start countdown
        cx.spawn_in(window, async move |this, cx| {
            for i in (0..=BUNKER_TIMEOUT).rev() {
                if i == 0 {
                    this.update(cx, |this, cx| {
                        this.set_countdown(None, cx);
                    })
                    .ok();
                } else {
                    this.update(cx, |this, cx| {
                        this.set_countdown(Some(i), cx);
                    })
                    .ok();
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();

        // Handle connection
        cx.spawn_in(window, async move |this, cx| {
            let result = signer.bunker_uri().await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(uri) => {
                        this.save_connection(&app_keys, &uri, window, cx);
                        this.connect(signer, cx);
                    }
                    Err(e) => {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    fn save_connection(
        &mut self,
        keys: &Keys,
        uri: &NostrConnectUri,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystore = KeyStore::global(cx).read(cx).backend();
        let username = keys.public_key().to_hex();
        let secret = keys.secret_key().to_secret_bytes();
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
        cx.background_spawn(async move {
            let client = client();
            client.set_signer(signer).await;
        })
        .detach();
    }

    pub fn login_with_password(&mut self, content: &str, pwd: &str, cx: &mut Context<Self>) {
        if pwd.is_empty() {
            self.set_error("Password is required", cx);
            return;
        }

        let Ok(enc) = EncryptedSecretKey::from_bech32(content) else {
            self.set_error("Secret Key is invalid", cx);
            return;
        };

        let password = pwd.to_owned();

        // Decrypt in the background to ensure it doesn't block the UI
        let task = cx.background_spawn(async move {
            if let Ok(content) = enc.decrypt(&password) {
                Ok(Keys::new(content))
            } else {
                Err(anyhow!("Invalid password"))
            }
        });

        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| {
                match result {
                    Ok(keys) => {
                        this.login_with_keys(keys, cx);
                    }
                    Err(e) => {
                        this.set_error(e.to_string(), cx);
                    }
                };
            })
            .ok();
        })
        .detach();
    }

    pub fn login_with_keys(&mut self, keys: Keys, cx: &mut Context<Self>) {
        let keystore = KeyStore::global(cx).read(cx).backend();

        let username = keys.public_key().to_hex();
        let secret = keys.secret_key().to_secret_hex().into_bytes();

        cx.spawn(async move |this, cx| {
            let bunker_url = KeyItem::User.to_string();

            // Write the app keys for further connection
            if let Err(e) = keystore
                .write_credentials(&bunker_url, &username, &secret, cx)
                .await
            {
                this.update(cx, |this, cx| {
                    this.set_error(e.to_string(), cx);
                })
                .ok();
            }

            // Update the signer
            cx.background_spawn(async move {
                let client = client();
                client.set_signer(keys).await;
            })
            .detach();
        })
        .detach();
    }

    fn set_error<S>(&mut self, message: S, cx: &mut Context<Self>)
    where
        S: Into<SharedString>,
    {
        // Reset the log in state
        self.set_logging_in(false, cx);

        // Reset the countdown
        self.set_countdown(None, cx);

        // Update error message
        self.error.update(cx, |this, cx| {
            *this = Some(message.into());
            cx.notify();
        });

        // Clear the error message after 3 secs
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;

            this.update(cx, |this, cx| {
                this.error.update(cx, |this, cx| {
                    *this = None;
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    fn set_logging_in(&mut self, status: bool, cx: &mut Context<Self>) {
        self.logging_in = status;
        cx.notify();
    }

    fn set_countdown(&mut self, i: Option<u64>, cx: &mut Context<Self>) {
        self.countdown.update(cx, |this, cx| {
            *this = i;
            cx.notify();
        });
    }
}

impl Panel for Login {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }
}

impl EventEmitter<PanelEvent> for Login {}

impl Focusable for Login {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Login {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .relative()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w_96()
                    .gap_10()
                    .child(
                        div()
                            .text_center()
                            .text_xl()
                            .font_semibold()
                            .line_height(relative(1.3))
                            .child(SharedString::from("Continue with Private Key or Bunker")),
                    )
                    .child(
                        v_flex()
                            .gap_3()
                            .text_sm()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .text_sm()
                                    .text_color(cx.theme().text_muted)
                                    .child("nsec or bunker://")
                                    .child(TextInput::new(&self.key_input)),
                            )
                            .when(self.require_password, |this| {
                                this.child(
                                    v_flex()
                                        .gap_1()
                                        .text_sm()
                                        .text_color(cx.theme().text_muted)
                                        .child("Password:")
                                        .child(TextInput::new(&self.pass_input)),
                                )
                            })
                            .child(
                                Button::new("login")
                                    .label("Continue")
                                    .primary()
                                    .loading(self.logging_in)
                                    .disabled(self.logging_in)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.login(window, cx);
                                    })),
                            )
                            .when_some(self.countdown.read(cx).as_ref(), |this, i| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_center()
                                        .text_color(cx.theme().text_muted)
                                        .child(SharedString::from(format!(
                                            "Approve connection request from your signer in {} seconds",
                                            i
                                        ))),
                                )
                            })
                            .when_some(self.error.read(cx).as_ref(), |this, error| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_center()
                                        .text_color(cx.theme().danger_foreground)
                                        .child(error.clone()),
                                )
                            }),
                    ),
            )
    }
}
