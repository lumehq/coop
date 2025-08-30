use common::display::{shorten_pubkey, ReadableProfile};
use common::nip05::nip05_verify;
use global::nostr_client;
use gpui::{
    div, relative, rems, App, AppContext, Context, Div, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use registry::Registry;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::{h_flex, v_flex, ContextModal, Icon, IconName, Sizable, StyledExt};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) -> Entity<Screening> {
    cx.new(|cx| Screening::new(public_key, window, cx))
}

pub struct Screening {
    profile: Profile,
    verified: bool,
    followed: bool,
    dm_relays: bool,
    mutual_contacts: usize,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Screening {
    pub fn new(public_key: PublicKey, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = Registry::read_global(cx);
        let identity = registry.identity(cx).public_key();
        let profile = registry.get_person(&public_key, cx);

        let mut tasks = smallvec![];

        let check_trust_score: Task<(bool, usize, bool)> = cx.background_spawn(async move {
            let client = nostr_client();

            let follow = Filter::new()
                .kind(Kind::ContactList)
                .author(identity)
                .pubkey(public_key)
                .limit(1);

            let contacts = Filter::new()
                .kind(Kind::ContactList)
                .pubkey(public_key)
                .limit(1);

            let relays = Filter::new()
                .kind(Kind::InboxRelays)
                .author(public_key)
                .limit(1);

            let is_follow = client.database().count(follow).await.unwrap_or(0) >= 1;
            let mutual_contacts = client.database().count(contacts).await.unwrap_or(0);
            let dm_relays = client.database().count(relays).await.unwrap_or(0) >= 1;

            (is_follow, mutual_contacts, dm_relays)
        });

        let verify_nip05 = if let Some(address) = profile.metadata().nip05 {
            Some(Tokio::spawn(cx, async move {
                nip05_verify(public_key, &address).await.unwrap_or(false)
            }))
        } else {
            None
        };

        tasks.push(
            // Load all necessary data
            cx.spawn_in(window, async move |this, cx| {
                let (followed, mutual_contacts, dm_relays) = check_trust_score.await;

                this.update(cx, |this, cx| {
                    this.followed = followed;
                    this.mutual_contacts = mutual_contacts;
                    this.dm_relays = dm_relays;
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
            verified: false,
            followed: false,
            dm_relays: false,
            mutual_contacts: 0,
            _tasks: tasks,
        }
    }

    fn address(&self, _cx: &Context<Self>) -> Option<String> {
        self.profile.metadata().nip05
    }

    fn open_njump(&mut self, _window: &mut Window, cx: &mut App) {
        let Ok(bech32) = self.profile.public_key().to_bech32();
        cx.open_url(&format!("https://njump.me/{bech32}"));
    }

    fn report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let public_key = self.profile.public_key();

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
                    window.push_notification(t!("screening.report_msg"), cx);
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for Screening {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let shorten_pubkey = shorten_pubkey(self.profile.public_key(), 8);

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(Avatar::new(self.profile.avatar_url(proxy)).size(rems(4.)))
                    .child(
                        div()
                            .font_semibold()
                            .line_height(relative(1.25))
                            .child(self.profile.display_name()),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .p_1()
                            .flex_1()
                            .h_7()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(cx.theme().surface_background)
                            .text_sm()
                            .truncate()
                            .text_ellipsis()
                            .text_center()
                            .line_height(relative(1.))
                            .child(shorten_pubkey),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("njump")
                                    .label(t!("profile.njump"))
                                    .secondary()
                                    .small()
                                    .rounded(ButtonRounded::Full)
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.open_njump(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("report")
                                    .tooltip(t!("screening.report"))
                                    .icon(IconName::Report)
                                    .danger()
                                    .rounded(ButtonRounded::Full)
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.report(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .text_sm()
                            .child(status_badge(self.followed, cx))
                            .child(
                                v_flex()
                                    .text_sm()
                                    .child(shared_t!("screening.contact_label"))
                                    .child(div().text_color(cx.theme().text_muted).child({
                                        if self.followed {
                                            shared_t!("screening.contact")
                                        } else {
                                            shared_t!("screening.not_contact")
                                        }
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .child(status_badge(self.verified, cx))
                            .child(
                                v_flex()
                                    .text_sm()
                                    .child({
                                        if let Some(addr) = self.address(cx) {
                                            shared_t!("screening.nip05_addr", addr = addr)
                                        } else {
                                            shared_t!("screening.nip05_label")
                                        }
                                    })
                                    .child(div().text_color(cx.theme().text_muted).child({
                                        if self.address(cx).is_some() {
                                            if self.verified {
                                                shared_t!("screening.nip05_ok")
                                            } else {
                                                shared_t!("screening.nip05_failed")
                                            }
                                        } else {
                                            shared_t!("screening.nip05_empty")
                                        }
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .child(status_badge(self.mutual_contacts > 0, cx))
                            .child(
                                v_flex()
                                    .text_sm()
                                    .child(shared_t!("screening.mutual_label"))
                                    .child(div().text_color(cx.theme().text_muted).child({
                                        if self.mutual_contacts > 0 {
                                            shared_t!("screening.mutual", u = self.mutual_contacts)
                                        } else {
                                            shared_t!("screening.no_mutual")
                                        }
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .child(status_badge(self.dm_relays, cx))
                            .child(
                                v_flex()
                                    .w_full()
                                    .text_sm()
                                    .child({
                                        if self.dm_relays {
                                            shared_t!("screening.relay_found")
                                        } else {
                                            shared_t!("screening.relay_empty")
                                        }
                                    })
                                    .child(div().w_full().text_color(cx.theme().text_muted).child(
                                        {
                                            if self.dm_relays {
                                                shared_t!("screening.relay_found_desc")
                                            } else {
                                                shared_t!("screening.relay_empty_desc")
                                            }
                                        },
                                    )),
                            ),
                    ),
            )
    }
}

fn status_badge(status: bool, cx: &App) -> Div {
    div()
        .pt_1()
        .flex_shrink_0()
        .child(Icon::new(IconName::CheckCircleFill).small().text_color({
            if status {
                cx.theme().icon_accent
            } else {
                cx.theme().icon_muted
            }
        }))
}
