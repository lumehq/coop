use common::display::DisplayProfile;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, relative, rems, App, AppContext, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window,
};
use i18n::t;
use identity::Identity;
use nostr::prelude::*;
use registry::Registry;
use settings::AppSettings;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::{v_flex, Sizable, StyledExt};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) -> Entity<UserProfile> {
    UserProfile::new(public_key, window, cx)
}

pub struct UserProfile {
    public_key: PublicKey,
    followed: bool,
    verified: bool,
}

impl UserProfile {
    pub fn new(public_key: PublicKey, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            public_key,
            followed: false,
            verified: false,
        })
    }

    pub fn on_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = self.public_key;
        let Some(identity) = Identity::read_global(cx).public_key() else {
            return;
        };

        let task: Task<bool> = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new()
                .kind(Kind::ContactList)
                .author(identity)
                .pubkey(public_key)
                .limit(1);

            client.database().count(filter).await.unwrap_or(0) >= 1
        });

        cx.spawn_in(window, async move |this, cx| {
            let followed = task.await;

            this.update(cx, |this, cx| {
                this.followed = followed;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        //
    }
}

impl Render for UserProfile {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;
        let registry = Registry::read_global(cx);
        let profile = registry.get_person(&self.public_key, cx);

        let Ok(bech32) = profile.public_key().to_bech32();
        let shared_bech32 = SharedString::new(bech32);

        v_flex()
            .px_4()
            .pt_8()
            .pb_4()
            .gap_4()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .child(Avatar::new(profile.avatar_url(proxy)).size(rems(4.)))
                    .text_center()
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .font_semibold()
                                    .line_height(relative(1.25))
                                    .child(profile.display_name()),
                            )
                            .when_some(profile.metadata().nip05, |this, nip05| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().text_muted)
                                        .child(nip05),
                                )
                            }),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .text_sm()
                    .child(div().text_color(cx.theme().text_muted).child("Public Key:"))
                    .child(
                        div()
                            .p_1p5()
                            .rounded_md()
                            .bg(cx.theme().elevated_surface_background)
                            .truncate()
                            .text_ellipsis()
                            .line_clamp(1)
                            .child(shared_bech32),
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
                    .when_some(profile.metadata().about, |this, bio| {
                        this.child(
                            div()
                                .p_1p5()
                                .rounded_md()
                                .bg(cx.theme().elevated_surface_background)
                                .child(bio),
                        )
                    }),
            )
            .child(
                Button::new("share-profile")
                    .label("Share Profile")
                    .primary()
                    .small()
                    .on_click(cx.listener(move |this, _e, window, cx| {
                        this.share(window, cx);
                    })),
            )
    }
}
