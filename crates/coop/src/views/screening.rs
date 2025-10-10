use std::time::Duration;

use app_state::constants::BOOTSTRAP_RELAYS;
use app_state::nostr_client;
use common::display::{shorten_pubkey, RenderedProfile, RenderedTimestamp};
use common::nip05::nip05_verify;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, App, AppContext, Context, Div, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use registry::Registry;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::indicator::Indicator;
use ui::{h_flex, v_flex, ContextModal, Icon, IconName, Sizable, StyledExt};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) -> Entity<Screening> {
    cx.new(|cx| Screening::new(public_key, window, cx))
}

pub struct Screening {
    profile: Profile,
    verified: bool,
    followed: bool,
    last_active: Option<Timestamp>,
    mutual_contacts: Vec<Profile>,
    _tasks: SmallVec<[Task<()>; 3]>,
}

impl Screening {
    pub fn new(public_key: PublicKey, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let registry = Registry::read_global(cx);
        let profile = registry.get_person(&public_key, cx);

        let mut tasks = smallvec![];

        let contact_check: Task<Result<(bool, Vec<Profile>), Error>> =
            cx.background_spawn(async move {
                let client = nostr_client();
                let signer = client.signer().await?;
                let signer_pubkey = signer.get_public_key().await?;

                // Check if user is in contact list
                let contacts = client.database().contacts_public_keys(signer_pubkey).await;
                let followed = contacts.unwrap_or_default().contains(&public_key);

                // Check mutual contacts
                let contact_list = Filter::new().kind(Kind::ContactList).pubkey(public_key);
                let mut mutual_contacts = vec![];

                if let Ok(events) = client.database().query(contact_list).await {
                    for event in events.into_iter().filter(|ev| ev.pubkey != signer_pubkey) {
                        if let Ok(metadata) = client.database().metadata(event.pubkey).await {
                            let profile = Profile::new(event.pubkey, metadata.unwrap_or_default());
                            mutual_contacts.push(profile);
                        }
                    }
                }

                Ok((followed, mutual_contacts))
            });

        let activity_check = cx.background_spawn(async move {
            let client = nostr_client();
            let filter = Filter::new().author(public_key).limit(1);
            let mut activity: Option<Timestamp> = None;

            if let Ok(mut stream) = client
                .stream_events_from(BOOTSTRAP_RELAYS, filter, Duration::from_secs(2))
                .await
            {
                while let Some(event) = stream.next().await {
                    activity = Some(event.created_at);
                }
            }

            activity
        });

        let addr_check = if let Some(address) = profile.metadata().nip05 {
            Some(Tokio::spawn(cx, async move {
                nip05_verify(public_key, &address).await.unwrap_or(false)
            }))
        } else {
            None
        };

        tasks.push(
            // Run the contact check in the background
            cx.spawn_in(window, async move |this, cx| {
                if let Ok((followed, mutual_contacts)) = contact_check.await {
                    this.update(cx, |this, cx| {
                        this.followed = followed;
                        this.mutual_contacts = mutual_contacts;
                        cx.notify();
                    })
                    .ok();
                }
            }),
        );

        tasks.push(
            // Run the activity check in the background
            cx.spawn_in(window, async move |this, cx| {
                let active = activity_check.await;

                this.update(cx, |this, cx| {
                    this.last_active = active;
                    cx.notify();
                })
                .ok();
            }),
        );

        tasks.push(
            // Run the NIP-05 verification in the background
            cx.spawn_in(window, async move |this, cx| {
                if let Some(task) = addr_check {
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
            last_active: None,
            mutual_contacts: vec![],
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

    fn mutual_contacts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let contacts = self.mutual_contacts.clone();

        window.open_modal(cx, move |this, _window, _cx| {
            let contacts = contacts.clone();
            let total = contacts.len();

            this.title(shared_t!("screening.mutual_label")).child(
                v_flex().gap_1().pb_4().child(
                    uniform_list("contacts", total, move |range, _window, cx| {
                        let mut items = Vec::with_capacity(total);

                        for ix in range {
                            if let Some(contact) = contacts.get(ix) {
                                items.push(
                                    h_flex()
                                        .h_11()
                                        .w_full()
                                        .px_2()
                                        .gap_1p5()
                                        .rounded(cx.theme().radius)
                                        .text_sm()
                                        .hover(|this| {
                                            this.bg(cx.theme().elevated_surface_background)
                                        })
                                        .child(Avatar::new(contact.avatar(true)).size(rems(1.75)))
                                        .child(contact.display_name()),
                                );
                            }
                        }

                        items
                    })
                    .h(px(300.)),
                ),
            )
        });
    }
}

impl Render for Screening {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let shorten_pubkey = shorten_pubkey(self.profile.public_key(), 8);
        let total_mutuals = self.mutual_contacts.len();
        let last_active = self.last_active.map(|_| true);

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(Avatar::new(self.profile.avatar(proxy)).size(rems(4.)))
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
                        h_flex()
                            .p_1()
                            .flex_1()
                            .h_7()
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
                                    .rounded()
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.open_njump(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("report")
                                    .tooltip(t!("screening.report"))
                                    .icon(IconName::Report)
                                    .danger()
                                    .rounded()
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
                            .child(status_badge(Some(self.followed), cx))
                            .child(
                                v_flex()
                                    .text_sm()
                                    .child(shared_t!("screening.contact_label"))
                                    .child(
                                        div()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .child({
                                                if self.followed {
                                                    shared_t!("screening.contact")
                                                } else {
                                                    shared_t!("screening.not_contact")
                                                }
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .text_sm()
                            .child(status_badge(last_active, cx))
                            .child(
                                v_flex()
                                    .text_sm()
                                    .child(
                                        h_flex()
                                            .gap_0p5()
                                            .child(shared_t!("screening.active_label"))
                                            .child(
                                                Button::new("active")
                                                    .icon(IconName::Info)
                                                    .xsmall()
                                                    .ghost()
                                                    .rounded()
                                                    .tooltip(t!("screening.active_tooltip")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .map(|this| {
                                                if let Some(date) = self.last_active {
                                                    this.child(shared_t!(
                                                        "screening.active_at",
                                                        d = date.to_human_time()
                                                    ))
                                                } else {
                                                    this.child(shared_t!("screening.no_active"))
                                                }
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .child(status_badge(Some(self.verified), cx))
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
                                    .child(
                                        div()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .child({
                                                if self.address(cx).is_some() {
                                                    if self.verified {
                                                        shared_t!("screening.nip05_ok")
                                                    } else {
                                                        shared_t!("screening.nip05_failed")
                                                    }
                                                } else {
                                                    shared_t!("screening.nip05_empty")
                                                }
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .child(status_badge(Some(total_mutuals > 0), cx))
                            .child(
                                v_flex()
                                    .text_sm()
                                    .child(
                                        h_flex()
                                            .gap_0p5()
                                            .child(shared_t!("screening.mutual_label"))
                                            .child(
                                                Button::new("mutuals")
                                                    .icon(IconName::Info)
                                                    .xsmall()
                                                    .ghost()
                                                    .rounded()
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.mutual_contacts(window, cx);
                                                        },
                                                    )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .child({
                                                if total_mutuals > 0 {
                                                    shared_t!("screening.mutual", u = total_mutuals)
                                                } else {
                                                    shared_t!("screening.no_mutual")
                                                }
                                            }),
                                    ),
                            ),
                    ),
            )
    }
}

fn status_badge(status: Option<bool>, cx: &App) -> Div {
    h_flex()
        .size_6()
        .justify_center()
        .flex_shrink_0()
        .map(|this| {
            if let Some(status) = status {
                this.child(Icon::new(IconName::CheckCircleFill).small().text_color({
                    if status {
                        cx.theme().icon_accent
                    } else {
                        cx.theme().icon_muted
                    }
                }))
            } else {
                this.child(Indicator::new().small())
            }
        })
}
