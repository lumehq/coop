use common::display::DisplayProfile;
use common::nip05::nip05_verify;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, App, AppContext, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window,
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
use ui::{h_flex, v_flex, ContextModal, Icon, IconName, Sizable, StyledExt};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) -> Entity<Screening> {
    Screening::new(public_key, window, cx)
}

pub struct Screening {
    public_key: PublicKey,
    followed: bool,
    connections: usize,
    verified: bool,
}

impl Screening {
    pub fn new(public_key: PublicKey, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            public_key,
            followed: false,
            connections: 0,
            verified: false,
        })
    }

    pub fn on_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Skip if user isn't logged in
        let Some(identity) = Identity::read_global(cx).public_key() else {
            return;
        };

        let public_key = self.public_key;

        let check_trust_score: Task<(bool, usize)> = cx.background_spawn(async move {
            let client = nostr_client();

            let follow = Filter::new()
                .kind(Kind::ContactList)
                .author(identity)
                .pubkey(public_key)
                .limit(1);

            let connection = Filter::new()
                .kind(Kind::ContactList)
                .pubkey(public_key)
                .limit(1);

            let is_follow = client.database().count(follow).await.unwrap_or(0) >= 1;
            let connects = client.database().count(connection).await.unwrap_or(0);

            (is_follow, connects)
        });

        let verify_nip05 = if let Some(address) = self.address(cx) {
            Some(Tokio::spawn(cx, async move {
                nip05_verify(public_key, &address).await.unwrap_or(false)
            }))
        } else {
            None
        };

        cx.spawn_in(window, async move |this, cx| {
            let (followed, connections) = check_trust_score.await;

            this.update(cx, |this, cx| {
                this.followed = followed;
                this.connections = connections;
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

    fn open_njump(&mut self, _window: &mut Window, cx: &mut App) {
        let Ok(bech32) = self.public_key.to_bech32();
        cx.open_url(&format!("https://njump.me/{bech32}"));
    }

    fn report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = self.public_key;
        let task: Task<Result<(), Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let builder = EventBuilder::report(
                vec![Tag::public_key_report(public_key, Report::Impersonation)],
                "scam/impersonation",
            );
            let _ = client.send_event_builder(builder).await?;

            Ok(())
        });

        cx.spawn_in(window, async move |_, cx| {
            if task.await.is_ok() {
                cx.update(|window, cx| {
                    window.close_modal(cx);
                    window.push_notification("Report submitted successfully", cx);
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for Screening {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;
        let profile = self.profile(cx);

        let Ok(bech32) = profile.public_key().to_bech32();
        let shared_bech32 = SharedString::new(bech32);

        v_flex()
            .w_full()
            .px_4()
            .pt_8()
            .pb_4()
            .gap_4()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(4.)))
                    .child(
                        div()
                            .font_semibold()
                            .line_height(relative(1.25))
                            .child(profile.display_name()),
                    ),
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
                                    .p_1p5()
                                    .h_9()
                                    .rounded_md()
                                    .bg(cx.theme().elevated_surface_background)
                                    .truncate()
                                    .text_ellipsis()
                                    .text_sm()
                                    .child(shared_bech32),
                            )
                            .child(
                                h_flex()
                                    .child(
                                        Button::new("njump")
                                            .tooltip(t!("profile.njump"))
                                            .icon(IconName::OpenUrl)
                                            .ghost()
                                            .on_click(cx.listener(move |this, _e, window, cx| {
                                                this.open_njump(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("report")
                                            .tooltip("Report as a scam or impostor")
                                            .icon(IconName::Info)
                                            .ghost()
                                            .on_click(cx.listener(move |this, _e, window, cx| {
                                                this.report(window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .when_some(self.address(cx), |this, address| {
                        this.child(h_flex().gap_2().map(|this| {
                            if self.verified {
                                this.text_sm()
                                    .child(
                                        Icon::new(IconName::CheckCircleFill)
                                            .small()
                                            .flex_shrink_0()
                                            .text_color(cx.theme().icon_accent),
                                    )
                                    .child(div().flex_1().child(SharedString::new(t!(
                                        "screening.verified",
                                        address = address
                                    ))))
                            } else {
                                this.text_sm()
                                    .child(
                                        Icon::new(IconName::CheckCircleFill)
                                            .small()
                                            .text_color(cx.theme().icon_muted),
                                    )
                                    .child(div().flex_1().child(SharedString::new(t!(
                                        "screening.not_verified",
                                        address = address
                                    ))))
                            }
                        }))
                    })
                    .child(h_flex().gap_2().map(|this| {
                        if !self.followed {
                            this.text_sm()
                                .child(
                                    Icon::new(IconName::CheckCircleFill)
                                        .small()
                                        .text_color(cx.theme().icon_muted),
                                )
                                .child(SharedString::new(t!("screening.not_contact")))
                        } else {
                            this.text_sm()
                                .child(
                                    Icon::new(IconName::CheckCircleFill)
                                        .small()
                                        .text_color(cx.theme().icon_accent),
                                )
                                .child(SharedString::new(t!("screening.contact")))
                        }
                    }))
                    .child(
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .child(
                                Icon::new(IconName::CheckCircleFill)
                                    .small()
                                    .flex_shrink_0()
                                    .text_color({
                                        if self.connections > 0 {
                                            cx.theme().icon_accent
                                        } else {
                                            cx.theme().icon_muted
                                        }
                                    }),
                            )
                            .map(|this| {
                                if self.connections > 0 {
                                    this.child(SharedString::new(t!(
                                        "screening.total_connections",
                                        u = self.connections
                                    )))
                                } else {
                                    this.child(SharedString::new(t!("screening.no_connections")))
                                }
                            }),
                    ),
            )
    }
}
