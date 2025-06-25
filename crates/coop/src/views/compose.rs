use std::ops::Range;
use std::time::Duration;

use anyhow::{anyhow, Error};
use chats::room::{Room, RoomKind};
use chats::ChatRegistry;
use common::profile::RenderProfile;
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, impl_internal_actions, px, red, relative, uniform_list, App, AppContext, Context,
    Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, TextAlign, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::Timer;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::{ContextModal, Disableable, Icon, IconName, Sizable, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Compose> {
    cx.new(|cx| Compose::new(window, cx))
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct SelectContact(PublicKey);

impl_internal_actions!(contacts, [SelectContact]);

#[derive(Debug, Clone)]
struct Contact {
    profile: Profile,
    select: bool,
}

impl AsRef<Profile> for Contact {
    fn as_ref(&self) -> &Profile {
        &self.profile
    }
}

impl Contact {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            select: false,
        }
    }

    pub fn select(mut self) -> Self {
        self.select = true;
        self
    }
}

pub struct Compose {
    /// Input for the room's subject
    title_input: Entity<InputState>,
    /// Input for the room's members
    user_input: Entity<InputState>,
    /// The current user's contacts
    contacts: Vec<Entity<Contact>>,
    /// Input error message
    error_message: Entity<Option<SharedString>>,
    adding: bool,
    submitting: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 1]>,
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let user_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("npub or nprofile..."));

        let title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Family...(Optional)"));

        let error_message = cx.new(|_| None);
        let mut subscriptions = smallvec![];

        // Handle Enter event for user input
        subscriptions.push(cx.subscribe_in(
            &user_input,
            window,
            move |this, _input, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.add_and_select_contact(window, cx),
                    InputEvent::Change(_) => {}
                    _ => {}
                };
            },
        ));

        let get_contacts: Task<Result<Vec<Contact>, Error>> = cx.background_spawn(async move {
            let client = shared_state().client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let profiles = client.database().contacts(public_key).await?;
            let contacts = profiles.into_iter().map(Contact::new).collect_vec();

            Ok(contacts)
        });

        cx.spawn_in(window, async move |this, cx| {
            match get_contacts.await {
                Ok(contacts) => {
                    this.update(cx, |this, cx| {
                        this.contacts(contacts, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(e.to_string()).title("Contacts"),
                            cx,
                        );
                    })
                    .ok();
                }
            };
        })
        .detach();

        Self {
            adding: false,
            submitting: false,
            contacts: vec![],
            title_input,
            user_input,
            error_message,
            subscriptions,
        }
    }

    pub fn compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let public_keys: Vec<PublicKey> = self.selected(cx);

        if public_keys.is_empty() {
            self.set_error(Some("You need to add at least 1 receiver".into()), cx);
            return;
        }

        // Show loading spinner
        self.set_submitting(true, cx);

        // Convert selected pubkeys into Nostr tags
        let mut tag_list: Vec<Tag> = public_keys.iter().map(|pk| Tag::public_key(*pk)).collect();

        // Add subject if it is present
        if !self.title_input.read(cx).value().is_empty() {
            tag_list.push(Tag::custom(
                TagKind::Subject,
                vec![self.title_input.read(cx).value().to_string()],
            ));
        }

        let event: Task<Result<Room, anyhow::Error>> = cx.background_spawn(async move {
            let signer = shared_state().client().signer().await?;
            let public_key = signer.get_public_key().await?;

            let room = EventBuilder::private_msg_rumor(public_keys[0], "")
                .tags(Tags::from_list(tag_list))
                .build(public_key)
                .sign(&Keys::generate())
                .await
                .map(|event| Room::new(&event).kind(RoomKind::Ongoing))?;

            Ok(room)
        });

        cx.spawn_in(window, async move |this, cx| match event.await {
            Ok(room) => {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.set_submitting(false, cx);
                    })
                    .ok();

                    ChatRegistry::global(cx).update(cx, |this, cx| {
                        this.push_room(cx.new(|_| room), cx);
                    });

                    window.close_modal(cx);
                })
                .ok();
            }
            Err(e) => {
                this.update(cx, |this, cx| {
                    this.set_error(Some(e.to_string().into()), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn contacts(&mut self, contacts: impl IntoIterator<Item = Contact>, cx: &mut Context<Self>) {
        self.contacts
            .extend(contacts.into_iter().map(|contact| cx.new(|_| contact)));
        cx.notify();
    }

    fn push_contact(&mut self, contact: Contact, cx: &mut Context<Self>) {
        if !self
            .contacts
            .iter()
            .any(|e| e.read(cx).profile.public_key() == contact.profile.public_key())
        {
            self.contacts.insert(0, cx.new(|_| contact));
            cx.notify();
        }
    }

    fn selected(&self, cx: &Context<Self>) -> Vec<PublicKey> {
        self.contacts
            .iter()
            .filter_map(|contact| {
                if contact.read(cx).select {
                    Some(contact.read(cx).profile.public_key())
                } else {
                    None
                }
            })
            .collect()
    }

    fn add_and_select_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.user_input.read(cx).value().to_string();

        self.set_adding(true, cx);
        self.user_input.update(cx, |this, cx| {
            this.set_loading(true, cx);
        });

        let task: Task<Result<Contact, anyhow::Error>> = if content.contains("@") {
            cx.background_spawn(async move {
                let (tx, rx) = oneshot::channel::<Nip05Profile>();

                nostr_sdk::async_utility::task::spawn(async move {
                    if let Ok(profile) = nip05::profile(&content, None).await {
                        tx.send(profile).ok();
                    }
                });

                if let Ok(profile) = rx.await {
                    let public_key = profile.public_key;

                    let metadata = shared_state()
                        .client()
                        .fetch_metadata(public_key, Duration::from_secs(2))
                        .await?
                        .unwrap_or_default();

                    let profile = Profile::new(public_key, metadata);
                    let contact = Contact::new(profile).select();

                    Ok(contact)
                } else {
                    Err(anyhow!("Profile not found"))
                }
            })
        } else if content.starts_with("nprofile1") {
            let Some(public_key) = Nip19Profile::from_bech32(&content)
                .map(|nip19| nip19.public_key)
                .ok()
            else {
                self.set_error(Some("Public Key is not valid".into()), cx);
                return;
            };

            cx.background_spawn(async move {
                let metadata = shared_state()
                    .client()
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?
                    .unwrap_or_default();

                let profile = Profile::new(public_key, metadata);
                let contact = Contact::new(profile).select();

                Ok(contact)
            })
        } else {
            let Ok(public_key) = PublicKey::parse(&content) else {
                self.set_error(Some("Public Key is not valid".into()), cx);
                return;
            };

            cx.background_spawn(async move {
                let metadata = shared_state()
                    .client()
                    .fetch_metadata(public_key, Duration::from_secs(2))
                    .await?
                    .unwrap_or_default();

                let profile = Profile::new(public_key, metadata);
                let contact = Contact::new(profile).select();

                Ok(contact)
            })
        };

        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(contact) => {
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.push_contact(contact, cx);
                        this.set_adding(false, cx);
                        this.user_input.update(cx, |this, cx| {
                            this.set_value("", window, cx);
                            this.set_loading(false, cx);
                        });
                    })
                    .ok();
                })
                .ok();
            }
            Err(e) => {
                this.update(cx, |this, cx| {
                    this.set_error(Some(e.to_string().into()), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn set_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        if self.adding {
            self.set_adding(false, cx);
        }

        self.user_input.update(cx, |this, cx| {
            this.set_loading(false, cx);
        });

        // Update error message
        self.error_message.update(cx, |this, cx| {
            *this = error;
            cx.notify();
        });

        // Dismiss error after 2 seconds
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_secs(2)).await;
            this.update(cx, |this, cx| {
                this.set_error(None, cx);
            })
            .ok();
        })
        .detach();
    }

    fn set_adding(&mut self, status: bool, cx: &mut Context<Self>) {
        self.adding = status;
        cx.notify();
    }

    fn set_submitting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.submitting = status;
        cx.notify();
    }

    fn list_items(&self, range: Range<usize>, cx: &Context<Self>) -> Vec<impl IntoElement> {
        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;
        let mut items = Vec::with_capacity(self.contacts.len());

        for ix in range {
            let Some(entity) = self.contacts.get(ix).cloned() else {
                continue;
            };

            let profile = entity.read(cx).as_ref();
            let selected = entity.read(cx).select;

            items.push(
                div()
                    .id(ix)
                    .w_full()
                    .h_10()
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .child(img(profile.render_avatar(proxy)).size_7().flex_shrink_0())
                            .child(profile.render_name()),
                    )
                    .when(selected, |this| {
                        this.child(
                            Icon::new(IconName::CheckCircleFill)
                                .small()
                                .text_color(cx.theme().icon_accent),
                        )
                    })
                    .hover(|this| this.bg(cx.theme().elevated_surface_background))
                    .on_click(cx.listener(move |_this, _event, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.select = !this.select;
                            cx.notify();
                        });
                    })),
            );
        }

        items
    }
}

impl Render for Compose {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const DESCRIPTION: &str =
            "Start a conversation with someone using their npub or NIP-05 (like foo@bar.com).";

        let label: SharedString = if self.contacts.len() > 1 {
            "Create Group DM".into()
        } else {
            "Create DM".into()
        };

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_3()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(DESCRIPTION),
            )
            .when_some(self.error_message.read(cx).as_ref(), |this, msg| {
                this.child(div().px_3().text_xs().text_color(red()).child(msg.clone()))
            })
            .child(
                div().px_3().flex().flex_col().child(
                    div()
                        .h_10()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_sm().font_semibold().child("Subject:"))
                        .child(TextInput::new(&self.title_input).small().appearance(false)),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .mt_1()
                    .child(
                        div()
                            .px_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().font_semibold().child("To:"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(TextInput::new(&self.user_input).small())
                                    .child(
                                        Button::new("add-user")
                                            .icon(IconName::PlusCircleFill)
                                            .small()
                                            .ghost()
                                            .disabled(self.adding)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.add_and_select_contact(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .map(|this| {
                        if self.contacts.is_empty() {
                            this.child(
                                div()
                                    .w_full()
                                    .h_24()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .text_align(TextAlign::Center)
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .line_height(relative(1.2))
                                            .child("No contacts"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().text_muted)
                                            .child("Your recently contacts will appear here."),
                                    ),
                            )
                        } else {
                            this.child(
                                uniform_list(
                                    "contacts",
                                    self.contacts.len(),
                                    cx.processor(move |this, range, _window, cx| {
                                        this.list_items(range, cx)
                                    }),
                                )
                                .pb_4()
                                .min_h(px(280.)),
                            )
                        }
                    }),
            )
            .child(
                div().p_3().child(
                    Button::new("create_dm_btn")
                        .label(label)
                        .primary()
                        .w_full()
                        .loading(self.submitting)
                        .disabled(self.submitting || self.adding)
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.compose(window, cx);
                        })),
                ),
            )
    }
}
