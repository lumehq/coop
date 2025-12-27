use std::time::Duration;

use common::{nip05_verify, shorten_pubkey, RenderedProfile};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, App, AppContext, ClipboardItem, Context, Entity, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window,
};
use gpui_tokio::Tokio;
use nostr_sdk::prelude::*;
use person::PersonRegistry;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use state::client;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) -> Entity<ProfileViewer> {
    cx.new(|cx| ProfileViewer::new(public_key, window, cx))
}

#[derive(Debug)]
pub struct ProfileViewer {
    profile: Profile,

    /// Follow status
    followed: bool,

    /// Verification status
    verified: bool,

    /// Copy status
    copied: bool,

    /// Async operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl ProfileViewer {
    pub fn new(target: PublicKey, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let persons = PersonRegistry::global(cx);
        let profile = persons.read(cx).get(&target, cx);

        let mut tasks = smallvec![];

        let check_follow: Task<Result<bool, Error>> = cx.background_spawn(async move {
            let client = client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let contact_list = client.database().contacts_public_keys(public_key).await?;

            Ok(contact_list.contains(&target))
        });

        let verify_nip05 = if let Some(address) = profile.metadata().nip05 {
            Some(Tokio::spawn(cx, async move {
                nip05_verify(target, &address).await.unwrap_or(false)
            }))
        } else {
            None
        };

        tasks.push(
            // Load user profile data
            cx.spawn_in(window, async move |this, cx| {
                let followed = check_follow.await.unwrap_or(false);

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
            }),
        );

        Self {
            profile,
            followed: false,
            verified: false,
            copied: false,
            _tasks: tasks,
        }
    }

    fn address(&self, _cx: &Context<Self>) -> Option<String> {
        self.profile.metadata().nip05
    }

    fn copy_pubkey(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(bech32) = self.profile.public_key().to_bech32();
        let item = ClipboardItem::new_string(bech32);
        cx.write_to_clipboard(item);

        self.set_copied(true, window, cx);
    }

    fn set_copied(&mut self, status: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.copied = status;
        cx.notify();

        if status {
            self._tasks.push(
                // Reset the copied state after a delay
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.set_copied(false, window, cx);
                        })
                        .ok();
                    })
                    .ok();
                }),
            );
        }
    }
}

impl Render for ProfileViewer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let bech32 = shorten_pubkey(self.profile.public_key(), 16);
        let shared_bech32 = SharedString::from(bech32);

        v_flex()
            .gap_4()
            .text_sm()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(Avatar::new(self.profile.avatar(proxy)).size(rems(4.)))
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .font_semibold()
                                    .line_height(relative(1.25))
                                    .child(self.profile.display_name()),
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
                                .child(SharedString::from("Unknown contact")),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap_1()
                    .text_sm()
                    .child(
                        div()
                            .text_color(cx.theme().text_muted)
                            .child(SharedString::from("Bio:")),
                    )
                    .child(
                        div()
                            .p_2()
                            .min_h_16()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().elevated_surface_background)
                            .child(
                                self.profile
                                    .metadata()
                                    .about
                                    .map(SharedString::from)
                                    .unwrap_or(SharedString::from("No bio.")),
                            ),
                    ),
            )
            .child(div().my_1().h_px().w_full().bg(cx.theme().border))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().text_placeholder)
                            .font_semibold()
                            .child(SharedString::from("Public Key:")),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .w_full()
                            .h_12()
                            .justify_center()
                            .bg(cx.theme().surface_background)
                            .rounded(cx.theme().radius)
                            .text_sm()
                            .child(shared_bech32)
                            .child(
                                Button::new("copy")
                                    .icon({
                                        if self.copied {
                                            IconName::CheckCircleFill
                                        } else {
                                            IconName::Copy
                                        }
                                    })
                                    .xsmall()
                                    .ghost()
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.copy_pubkey(window, cx);
                                    })),
                            ),
                    ),
            )
    }
}
