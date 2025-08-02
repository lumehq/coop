use std::time::Duration;

use common::display::DisplayProfile;
use common::nip05::nip05_verify;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, App, AppContext, ClipboardItem, Context, Entity, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::t;
use identity::Identity;
use nostr_sdk::prelude::*;
use registry::Registry;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::{h_flex, v_flex, Disableable, Icon, IconName, Sizable, StyledExt};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) -> Entity<UserProfile> {
    UserProfile::new(public_key, window, cx)
}

pub struct UserProfile {
    public_key: PublicKey,
    followed: bool,
    verified: bool,
    copied: bool,
}

impl UserProfile {
    pub fn new(public_key: PublicKey, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            public_key,
            followed: false,
            verified: false,
            copied: false,
        })
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Skip if user isn't logged in
        let Some(identity) = Identity::read_global(cx).public_key() else {
            return;
        };

        let public_key = self.public_key;

        let check_follow: Task<bool> = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new()
                .kind(Kind::ContactList)
                .author(identity)
                .pubkey(public_key)
                .limit(1);

            client.database().count(filter).await.unwrap_or(0) >= 1
        });

        let verify_nip05 = if let Some(address) = self.address(cx) {
            Some(Tokio::spawn(cx, async move {
                nip05_verify(public_key, &address).await.unwrap_or(false)
            }))
        } else {
            None
        };

        cx.spawn_in(window, async move |this, cx| {
            let followed = check_follow.await;

            // Update the followed status
            this.update(cx, |this, cx| {
                this.followed = followed;
                cx.notify();
            })
            .ok();

            // Update the NIP05 verification status if user has NIP05 address
            if let Some(task) = verify_nip05 {
                if let Ok(verified) = task.await {
                    this.update(cx, |this, cx| {
                        this.verified = verified;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn profile(&self, cx: &Context<Self>) -> Profile {
        let registry = Registry::read_global(cx);
        registry.get_person(&self.public_key, cx)
    }

    fn address(&self, cx: &Context<Self>) -> Option<String> {
        self.profile(cx).metadata().nip05
    }

    fn copy_pubkey(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(bech32) = self.public_key.to_bech32();
        let item = ClipboardItem::new_string(bech32);
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
}

impl Render for UserProfile {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let profile = self.profile(cx);

        let Ok(bech32) = profile.public_key().to_bech32();
        let shared_bech32 = SharedString::new(bech32);

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(4.)))
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .font_semibold()
                                    .line_height(relative(1.25))
                                    .child(profile.display_name()),
                            )
                            .when_some(self.address(cx), |this, address| {
                                this.child(
                                    h_flex()
                                        .justify_center()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(cx.theme().text_muted)
                                        .child(address)
                                        .when(self.verified, |this| {
                                            this.child(
                                                div()
                                                    .relative()
                                                    .text_color(cx.theme().text_accent)
                                                    .child(
                                                        Icon::new(IconName::CheckCircleFill)
                                                            .small()
                                                            .block(),
                                                    ),
                                            )
                                        }),
                                )
                            }),
                    )
                    .when(!self.followed, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .w_32()
                                .p_1()
                                .rounded_full()
                                .bg(cx.theme().elevated_surface_background)
                                .text_xs()
                                .font_semibold()
                                .child(SharedString::new(t!("profile.unknown"))),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap_1()
                    .text_sm()
                    .child(
                        div()
                            .block()
                            .text_color(cx.theme().text_muted)
                            .child("Public Key:"),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                div()
                                    .p_2()
                                    .h_9()
                                    .rounded_md()
                                    .bg(cx.theme().elevated_surface_background)
                                    .truncate()
                                    .text_ellipsis()
                                    .line_clamp(1)
                                    .child(shared_bech32),
                            )
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
                                        this.copy_pubkey(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .text_sm()
                    .child(
                        div()
                            .text_color(cx.theme().text_muted)
                            .child(SharedString::new(t!("profile.label_bio"))),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded_md()
                            .bg(cx.theme().elevated_surface_background)
                            .child(
                                profile
                                    .metadata()
                                    .about
                                    .unwrap_or(t!("profile.no_bio").to_string()),
                            ),
                    ),
            )
    }
}
