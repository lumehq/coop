use std::time::Duration;

use anyhow::{anyhow, Error};
use common::home_dir;
use gpui::{
    div, App, AppContext, ClipboardItem, Context, Entity, Flatten, IntoElement, ParentElement,
    Render, SharedString, Styled, Task, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::divider::Divider;
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, IconName, StyledExt};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};

pub fn init(keys: &Keys, window: &mut Window, cx: &mut App) -> Entity<Backup> {
    cx.new(|cx| Backup::new(keys, window, cx))
}

#[derive(Debug)]
pub struct Backup {
    pubkey_input: Entity<InputState>,
    secret_input: Entity<InputState>,
    error: Option<SharedString>,
    copied: bool,

    // Async operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Backup {
    pub fn new(keys: &Keys, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Ok(npub) = keys.public_key.to_bech32();
        let Ok(nsec) = keys.secret_key().to_bech32();

        let pubkey_input = cx.new(|cx| InputState::new(window, cx).default_value(npub));
        let secret_input = cx.new(|cx| InputState::new(window, cx).default_value(nsec));

        Self {
            pubkey_input,
            secret_input,
            error: None,
            copied: false,
            _tasks: smallvec![],
        }
    }

    pub fn backup(&self, window: &Window, cx: &Context<Self>) -> Task<Result<(), Error>> {
        let dir = home_dir();
        let path = cx.prompt_for_new_path(dir, Some("My Nostr Account"));
        let nsec = self.secret_input.read(cx).value().to_string();

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(path.await.map_err(|e| e.into())) {
                Ok(Ok(Some(path))) => {
                    if let Err(e) = smol::fs::write(&path, nsec).await {
                        this.update_in(cx, |this, window, cx| {
                            this.set_error(e.to_string(), window, cx);
                        })
                        .expect("Entity has been released");
                    } else {
                        return Ok(());
                    }
                }
                _ => {
                    log::error!("Failed to save backup keys");
                }
            };

            Err(anyhow!("Failed to backup keys"))
        })
    }

    fn copy(&mut self, value: impl Into<String>, window: &mut Window, cx: &mut Context<Self>) {
        let item = ClipboardItem::new_string(value.into());
        cx.write_to_clipboard(item);

        self.set_copied(true, window, cx);
    }

    fn set_copied(&mut self, status: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.copied = status;
        cx.notify();

        // Reset the copied state after a delay
        if status {
            self._tasks.push(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;

                this.update_in(cx, |this, window, cx| {
                    this.set_copied(false, window, cx);
                })
                .ok();
            }));
        }
    }

    fn set_error<E>(&mut self, error: E, window: &mut Window, cx: &mut Context<Self>)
    where
        E: Into<SharedString>,
    {
        self.error = Some(error.into());
        cx.notify();

        // Clear the error message after a delay
        self._tasks.push(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;

            this.update(cx, |this, cx| {
                this.error = None;
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Render for Backup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const DESCRIPTION: &str = "In Nostr, your account is defined by a KEY PAIR. These keys are used to sign your messages and identify you.";
        const WARN: &str = "You must keep the Secret Key in a safe place. If you lose it, you will lose access to your account.";
        const PK: &str = "Public Key is the address that others will use to find you.";
        const SK: &str = "Secret Key provides access to your account.";

        v_flex()
            .gap_2()
            .text_sm()
            .child(SharedString::from(DESCRIPTION))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .font_semibold()
                            .child(SharedString::from("Public Key:")),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Input::new(&self.pubkey_input)
                                    .disabled(true)
                                    .bordered(false),
                            )
                            .child(
                                Button::new("copy-pubkey")
                                    .icon({
                                        if self.copied {
                                            IconName::CircleCheck
                                        } else {
                                            IconName::Copy
                                        }
                                    })
                                    .ghost()
                                    .disabled(self.copied)
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.copy(this.pubkey_input.read(cx).value(), window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(PK)),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .font_semibold()
                            .child(SharedString::from("Secret Key:")),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Input::new(&self.secret_input)
                                    .disabled(true)
                                    .bordered(false),
                            )
                            .child(
                                Button::new("copy-secret")
                                    .icon({
                                        if self.copied {
                                            IconName::CircleCheck
                                        } else {
                                            IconName::Copy
                                        }
                                    })
                                    .ghost()
                                    .disabled(self.copied)
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.copy(this.secret_input.read(cx).value(), window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(SK)),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(SharedString::from(WARN)),
            )
    }
}
