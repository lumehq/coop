use std::time::Duration;

use common::{nip05_verify, shorten_pubkey, RenderedProfile, RenderedTimestamp, BOOTSTRAP_RELAYS};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, App, AppContext, Context, Div, Entity,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, Task, Window,
};
use gpui_tokio::Tokio;
use nostr_sdk::prelude::*;
use person::PersonRegistry;
use smallvec::{smallvec, SmallVec};
use state::client;
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
        let persons = PersonRegistry::global(cx);
        let profile = persons.read(cx).get(&public_key, cx);

        let mut tasks = smallvec![];

        let contact_check: Task<Result<(bool, Vec<Profile>), Error>> =
            cx.background_spawn(async move {
                let client = client();
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
            let client = client();
            let filter = Filter::new().author(public_key).limit(1);
            let mut activity: Option<Timestamp> = None;

            if let Ok(mut stream) = client
                .stream_events_from(BOOTSTRAP_RELAYS, filter, Duration::from_secs(2))
                .await
            {
                while let Some((_url, event)) = stream.next().await {
                    if let Ok(event) = event {
                        activity = Some(event.created_at);
                    }
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
            let client = client();
            let signer = client.signer().await?;
            let tag = Tag::public_key_report(public_key, Report::Impersonation);
            let event = EventBuilder::report(vec![tag], "").sign(&signer).await?;

            // Send the report to the public relays
            client.send_event(&event).await?;

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

    fn mutual_contacts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let contacts = self.mutual_contacts.clone();

        window.open_modal(cx, move |this, _window, _cx| {
            let contacts = contacts.clone();
            let total = contacts.len();

            this.title(SharedString::from("Mutual contacts")).child(
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
                                        .child(Avatar::new(contact.avatar()).size(rems(1.75)))
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
                    .child(Avatar::new(self.profile.avatar()).size(rems(4.)))
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
                                    .label("View on njump.me")
                                    .secondary()
                                    .small()
                                    .rounded()
                                    .on_click(cx.listener(move |this, _e, window, cx| {
                                        this.open_njump(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("report")
                                    .tooltip("Report as a scam or impostor")
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
                                    .child(SharedString::from("Contact"))
                                    .child(
                                        div()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .child({
                                                if self.followed {
                                                    SharedString::from("This person is one of your contacts.")
                                                } else {
                                                    SharedString::from("This person is not one of your contacts.")
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
                                            .child(SharedString::from("Activity on Public Relays"))
                                            .child(
                                                Button::new("active")
                                                    .icon(IconName::Info)
                                                    .xsmall()
                                                    .ghost()
                                                    .rounded()
                                                    .tooltip("This may be inaccurate if the user only publishes to their private relays."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .map(|this| {
                                                if let Some(date) = self.last_active {
                                                    this.child(SharedString::from(format!(
                                                        "Last active: {}.",
                                                        date.to_human_time()
                                                    )))
                                                } else {
                                                    this.child(SharedString::from("This person hasn't had any activity."))
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
                                            SharedString::from(format!("{} validation", addr))
                                        } else {
                                            SharedString::from("Friendly Address (NIP-05) validation")
                                        }
                                    })
                                    .child(
                                        div()
                                            .line_clamp(1)
                                            .text_color(cx.theme().text_muted)
                                            .child({
                                                if self.address(cx).is_some() {
                                                    if self.verified {
                                                        SharedString::from("The address matches the user's public key.")
                                                    } else {
                                                        SharedString::from("The address does not match the user's public key.")
                                                    }
                                                } else {
                                                    SharedString::from("This person has not set up their friendly address")
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
                                            .child(SharedString::from("Mutual contacts"))
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
                                                    SharedString::from(format!(
                                                        "You have {} mutual contacts with this person.",
                                                        total_mutuals
                                                    ))
                                                } else {
                                                    SharedString::from("You don't have any mutual contacts with this person.")
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
