use std::fs;
use std::time::Duration;

use common::home_dir;
use gpui::{
    div, AppContext, ClipboardItem, Context, Entity, Flatten, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window,
};
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputState, TextInput};
use ui::{divider, h_flex, v_flex, Disableable, IconName, Sizable};

pub struct BackupKeys {
    pubkey_input: Entity<InputState>,
    secret_input: Entity<InputState>,
    error: Option<SharedString>,
    copied: bool,
}

impl BackupKeys {
    pub fn new(keys: &Keys, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let Ok(npub) = keys.public_key.to_bech32();
        let Ok(nsec) = keys.secret_key().to_bech32();

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
            pubkey_input,
            secret_input,
            error: None,
            copied: false,
        }
    }

    pub fn backup(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Task<()>> {
        let dir = home_dir();
        let path = cx.prompt_for_new_path(dir, Some("My Nostr Account"));
        let nsec = self.secret_input.read(cx).value().to_string();

        Some(cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(path.await.map_err(|e| e.into())) {
                Ok(Ok(Some(path))) => {
                    cx.update(|window, cx| {
                        if let Err(e) = fs::write(&path, nsec) {
                            this.update(cx, |this, cx| {
                                this.set_error(e.to_string(), window, cx);
                            })
                            .ok();
                        }
                    })
                    .ok();
                }
                _ => {
                    log::error!("Failed to save backup keys");
                }
            };
        }))
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
    }
}
