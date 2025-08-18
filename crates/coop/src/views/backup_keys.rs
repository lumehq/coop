use std::fs;
use std::time::Duration;

use dirs::document_dir;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, AppContext, ClipboardItem, Context, Entity, Flatten, IntoElement, ParentElement, Render,
    SharedString, Styled, Window,
};
use i18n::{shared_t, t};
use identity::Identity;
use nostr_sdk::prelude::*;
use theme::ActiveTheme;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::{divider, h_flex, v_flex, ContextModal, Disableable, IconName, Sizable};

pub fn backup_button(keys: Keys) -> impl IntoElement {
    div().child(
        Button::new("backup")
            .icon(IconName::Info)
            .label(t!("new_account.backup_label"))
            .danger()
            .xsmall()
            .rounded(ButtonRounded::Full)
            .on_click(move |_, window, cx| {
                let title = SharedString::new(t!("new_account.backup_label"));
                let keys = keys.clone();
                let view = cx.new(|cx| BackupKeys::new(&keys, window, cx));
                let weak_view = view.downgrade();

                window.open_modal(cx, move |modal, _window, _cx| {
                    let weak_view = weak_view.clone();

                    modal
                        .confirm()
                        .title(title.clone())
                        .child(view.clone())
                        .button_props(
                            ModalButtonProps::default()
                                .cancel_text(t!("new_account.backup_skip"))
                                .ok_text(t!("new_account.backup_download")),
                        )
                        .on_ok(move |_, window, cx| {
                            weak_view
                                .update(cx, |this, cx| {
                                    this.download(window, cx);
                                })
                                .ok();
                            // true to close the modal
                            false
                        })
                })
            }),
    )
}

pub struct BackupKeys {
    password: Entity<InputState>,
    pubkey_input: Entity<InputState>,
    secret_input: Entity<InputState>,
    error: Option<SharedString>,
    copied: bool,
}

impl BackupKeys {
    pub fn new(keys: &Keys, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let Ok(npub) = keys.public_key.to_bech32();
        let Ok(nsec) = keys.secret_key().to_bech32();

        let password = cx.new(|cx| InputState::new(window, cx).masked(true));

        let pubkey_input = cx.new(|cx| {
            InputState::new(window, cx)
                .disabled(true)
                .default_value(npub)
        });

        let secret_input = cx.new(|cx| {
            InputState::new(window, cx)
                .disabled(true)
                .default_value(nsec)
        });

        Self {
            password,
            pubkey_input,
            secret_input,
            error: None,
            copied: false,
        }
    }

    fn copy_secret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let item = ClipboardItem::new_string(self.secret_input.read(cx).value().to_string());
        cx.write_to_clipboard(item);

        self.set_copied(true, window, cx);
    }

    fn set_copied(&mut self, status: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.copied = status;
        cx.notify();

        // Reset the copied state after a delay
        if status {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_copied(false, window, cx);
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
        }
    }

    fn set_error<E>(&mut self, error: E, window: &mut Window, cx: &mut Context<Self>)
    where
        E: Into<SharedString>,
    {
        self.error = Some(error.into());
        cx.notify();

        // Clear the error message after a delay
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            cx.update(|_, cx| {
                this.update(cx, |this, cx| {
                    this.error = None;
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    fn download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let document_dir = document_dir().expect("Failed to get document directory");
        let password = self.password.read(cx).value().to_string();

        if password.is_empty() {
            self.set_error(t!("login.password_is_required"), window, cx);
            return;
        };

        let path = cx.prompt_for_new_path(&document_dir, None);
        let nsec = self.secret_input.read(cx).value().to_string();

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(path.await.map_err(|e| e.into())) {
                Ok(Ok(Some(path))) => {
                    cx.update(|window, cx| {
                        match fs::write(&path, nsec) {
                            Ok(_) => {
                                Identity::global(cx).update(cx, |this, cx| {
                                    this.clear_need_backup(password, cx);
                                });
                                // Close the current modal
                                window.close_modal(cx);
                            }
                            Err(e) => {
                                this.update(cx, |this, cx| {
                                    this.set_error(e.to_string(), window, cx);
                                })
                                .ok();
                            }
                        };
                    })
                    .ok();
                }
                _ => {
                    log::error!("Failed to save backup keys");
                }
            };
        })
        .detach();
    }
}

impl Render for BackupKeys {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().text_muted)
                    .child(shared_t!("new_account.backup_description")),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(shared_t!("common.pubkey"))
                    .child(TextInput::new(&self.pubkey_input).small())
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child(shared_t!("new_account.backup_pubkey_note")),
                    ),
            )
            .child(divider(cx))
            .child(
                v_flex()
                    .gap_1()
                    .child(shared_t!("common.secret"))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(TextInput::new(&self.secret_input).small())
                            .child(
                                Button::new("copy")
                                    .icon({
                                        if self.copied {
                                            IconName::CheckCircleFill
                                        } else {
                                            IconName::Copy
                                        }
                                    })
                                    .ghost()
                                    .disabled(self.copied)
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.copy_secret(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child(shared_t!("new_account.backup_secret_note")),
                    ),
            )
            .child(divider(cx))
            .child(
                v_flex()
                    .gap_1()
                    .child(shared_t!("login.set_password"))
                    .child(TextInput::new(&self.password).small())
                    .when_some(self.error.as_ref(), |this, error| {
                        this.child(
                            div()
                                .italic()
                                .text_xs()
                                .text_color(cx.theme().danger_foreground)
                                .child(error.clone()),
                        )
                    }),
            )
    }
}
