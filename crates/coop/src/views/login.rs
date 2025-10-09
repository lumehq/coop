use std::time::Duration;

use global::app_state::AppIdentifierTag;
use global::constants::{ACCOUNT_PATH, BUNKER_TIMEOUT};
use global::{app_state, nostr_client};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task,
    Window,
};
use i18n::{shared_t, t};
use nostr_connect::prelude::*;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::{v_flex, ContextModal, Disableable, Sizable, StyledExt};

use crate::actions::CoopAuthUrlHandler;

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Login> {
    Login::new(window, cx)
}

pub struct Login {
    input: Entity<InputState>,
    error: Entity<Option<SharedString>>,
    countdown: Entity<Option<u64>>,
    logging_in: bool,

    // Panel
    name: SharedString,
    focus_handle: FocusHandle,

    // All subscriptions
    _subscriptions: SmallVec<[Subscription; 1]>,
}

impl Login {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("nsec... or bunker://..."));
        let error = cx.new(|_| None);
        let countdown = cx.new(|_| None);

        let mut subscriptions = smallvec![];

        // Subscribe to key input events and process login when the user presses enter
        subscriptions.push(
            cx.subscribe_in(&input, window, |this, _e, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.login(window, cx);
                }
            }),
        );

        Self {
            input,
            error,
            countdown,
            _subscriptions: subscriptions,
            name: "Login".into(),
            focus_handle: cx.focus_handle(),
            logging_in: false,
        }
    }

    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.logging_in {
            return;
        };

        // Prevent duplicate login requests
        self.set_logging_in(true, cx);

        // Disable the input
        self.input.update(cx, |this, cx| {
            this.set_loading(true, cx);
            this.set_disabled(true, cx);
        });

        // Content can be secret key or bunker://
        match self.input.read(cx).value().to_string() {
            s if s.starts_with("nsec1") => self.ask_for_password(s, window, cx),
            s if s.starts_with("ncryptsec1") => self.ask_for_password(s, window, cx),
            s if s.starts_with("bunker://") => self.login_with_bunker(s, window, cx),
            _ => self.set_error(t!("login.invalid_key"), window, cx),
        };
    }

    fn ask_for_password(&mut self, content: String, window: &mut Window, cx: &mut Context<Self>) {
        let current_view = cx.entity().downgrade();
        let is_ncryptsec = content.starts_with("ncryptsec1");

        let pwd_input = cx.new(|cx| InputState::new(window, cx).masked(true));
        let weak_pwd_input = pwd_input.downgrade();

        let confirm_input = cx.new(|cx| InputState::new(window, cx).masked(true));
        let weak_confirm_input = confirm_input.downgrade();

        window.open_modal(cx, move |this, _window, cx| {
            let weak_pwd_input = weak_pwd_input.clone();
            let weak_confirm_input = weak_confirm_input.clone();

            let view_cancel = current_view.clone();
            let view_ok = current_view.clone();

            let label: SharedString = if !is_ncryptsec {
                t!("login.set_password").into()
            } else {
                t!("login.password_to_decrypt").into()
            };

            let description: SharedString = if is_ncryptsec {
                t!("login.password_description").into()
            } else {
                t!("login.password_description_full").into()
            };

            this.overlay_closable(false)
                .show_close(false)
                .keyboard(false)
                .confirm()
                .on_cancel(move |_, window, cx| {
                    view_cancel
                        .update(cx, |this, cx| {
                            this.set_error(t!("login.password_is_required"), window, cx);
                        })
                        .ok();
                    true
                })
                .on_ok(move |_, window, cx| {
                    let value = weak_pwd_input
                        .read_with(cx, |state, _cx| state.value().to_owned())
                        .ok();

                    let confirm = weak_confirm_input
                        .read_with(cx, |state, _cx| state.value().to_owned())
                        .ok();

                    view_ok
                        .update(cx, |this, cx| {
                            this.verify_password(value, confirm, is_ncryptsec, window, cx);
                        })
                        .ok();
                    true
                })
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_sm()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(label)
                                .child(TextInput::new(&pwd_input).small()),
                        )
                        .when(content.starts_with("nsec1"), |this| {
                            this.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(SharedString::new(t!("login.confirm_password")))
                                    .child(TextInput::new(&confirm_input).small()),
                            )
                        })
                        .child(
                            div()
                                .text_xs()
                                .italic()
                                .text_color(cx.theme().text_placeholder)
                                .child(description),
                        ),
                )
        });
    }

    fn verify_password(
        &mut self,
        password: Option<SharedString>,
        confirm: Option<SharedString>,
        is_ncryptsec: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(password) = password else {
            self.set_error(t!("login.password_is_required"), window, cx);
            return;
        };

        if password.is_empty() {
            self.set_error(t!("login.password_is_required"), window, cx);
            return;
        }

        // Skip verification if key is ncryptsec
        if is_ncryptsec {
            self.login_with_keys(password.to_string(), window, cx);
            return;
        }

        let Some(confirm) = confirm else {
            self.set_error(t!("login.must_confirm_password"), window, cx);
            return;
        };

        if confirm.is_empty() {
            self.set_error(t!("login.must_confirm_password"), window, cx);
            return;
        }

        if password != confirm {
            self.set_error(t!("login.password_not_match"), window, cx);
            return;
        }

        self.login_with_keys(password.to_string(), window, cx);
    }

    fn login_with_keys(&mut self, password: String, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();

        let secret_key = if value.starts_with("nsec1") {
            SecretKey::parse(&value).ok()
        } else if value.starts_with("ncryptsec1") {
            EncryptedSecretKey::from_bech32(&value)
                .map(|enc| enc.decrypt(&password).ok())
                .unwrap_or_default()
        } else {
            None
        };

        if let Some(secret_key) = secret_key {
            let keys = Keys::new(secret_key);

            // Encrypt and save user secret key to disk
            self.write_keys_to_disk(&keys, password, cx);

            // Set the client's signer with the current keys
            cx.background_spawn(async move {
                let client = nostr_client();
                client.set_signer(keys).await;
            })
            .detach();
        } else {
            self.set_error(t!("login.key_invalid"), window, cx);
        }
    }

    fn login_with_bunker(&mut self, content: String, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(uri) = NostrConnectURI::parse(content) else {
            self.set_error(t!("login.bunker_invalid"), window, cx);
            return;
        };

        let timeout = Duration::from_secs(BUNKER_TIMEOUT);
        let keys = Keys::generate();

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
            let mut signer = NostrConnect::new(uri, keys.clone(), timeout, None).unwrap();

            // Automatically handle authentication url
            signer.auth_url_handler(CoopAuthUrlHandler);

            let result = signer.bunker_uri().await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(uri) => {
                        this.write_bunker_keys(keys.secret_key(), cx);
                        this.connect(signer, uri, cx);
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

    fn write_bunker_keys(&mut self, secret: &SecretKey, cx: &mut Context<Self>) {
        let secret_hex = secret.to_secret_hex();

        let task: Task<Result<(), anyhow::Error>> = cx.background_spawn(async move {
            let app_state = app_state();

            // Write the secret key that used for connection to the database for future logins
            app_state
                .write_to_db(&secret_hex, AppIdentifierTag::Bunker)
                .await?;

            Ok(())
        });

        task.detach();
    }

    fn connect(&mut self, signer: NostrConnect, uri: NostrConnectURI, cx: &mut Context<Self>) {
        let mut uri_without_secret = uri.to_string();

        // Clear the secret parameter in the URI if it exists
        if let Some(secret) = uri.secret() {
            uri_without_secret = uri_without_secret.replace(secret, "");
        }

        let task: Task<Result<(), anyhow::Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let app_state = app_state();

            // Update the client's signer
            client.set_signer(signer).await;

            // Write current user connection to the database for future logins
            app_state
                .write_to_db(&uri_without_secret, AppIdentifierTag::User)
                .await?;

            Ok(())
        });

        task.detach();
    }

    pub fn write_keys_to_disk(&self, keys: &Keys, password: String, cx: &mut Context<Self>) {
        let keys = keys.to_owned();
        let public_key = keys.public_key();

        cx.background_spawn(async move {
            if let Ok(enc_key) =
                EncryptedSecretKey::new(keys.secret_key(), &password, 8, KeySecurity::Unknown)
            {
                let client = nostr_client();
                let value = enc_key.to_bech32().unwrap();

                let builder = EventBuilder::new(Kind::ApplicationSpecificData, value)
                    .tag(Tag::identifier(ACCOUNT_PATH))
                    .build(public_key)
                    .sign(&Keys::generate())
                    .await;

                if let Ok(event) = builder {
                    if let Err(e) = client.database().save_event(&event).await {
                        log::error!("Failed to save event: {e}");
                    };
                }
            }
        })
        .detach();
    }

    fn set_error(
        &mut self,
        message: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Reset the log in state
        self.set_logging_in(false, cx);

        // Reset the countdown
        self.set_countdown(None, cx);

        // Update error message
        self.error.update(cx, |this, cx| {
            *this = Some(message.into());
            cx.notify();
        });

        // Re enable the input
        self.input.update(cx, |this, cx| {
            this.set_value("", window, cx);
            this.set_loading(false, cx);
            this.set_disabled(false, cx);
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

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
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
                            .child(
                                div()
                                    .text_xl()
                                    .font_semibold()
                                    .line_height(relative(1.3))
                                    .child(shared_t!("login.title")),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().text_muted)
                                    .child(shared_t!("login.key_description")),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_3()
                            .child(TextInput::new(&self.input))
                            .child(
                                Button::new("login")
                                    .label(t!("common.continue"))
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
                                        .child(shared_t!("login.approve_message", i = i)),
                                )
                            })
                            .when_some(self.error.read(cx).clone(), |this, error| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_center()
                                        .text_color(cx.theme().danger_foreground)
                                        .child(error),
                                )
                            }),
                    ),
            )
    }
}
