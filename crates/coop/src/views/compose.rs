use std::ops::Range;
use std::time::Duration;

use anyhow::{anyhow, Error};
use common::display::{DisplayProfile, TextUtils};
use common::nip05::nip05_profile;
use global::constants::BOOTSTRAP_RELAYS;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, AppContext, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, TextAlign, Window,
};
use i18n::t;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::room::{Room, RoomKind};
use registry::Registry;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::Timer;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::{h_flex, v_flex, ContextModal, Disableable, Icon, IconName, Sizable, StyledExt};

pub fn compose_button() -> impl IntoElement {
    div().child(
        Button::new("compose")
            .icon(IconName::Plus)
            .primary()
            .cta()
            .small()
            .rounded(ButtonRounded::Full)
            .on_click(move |_, window, cx| {
                let compose = cx.new(|cx| Compose::new(window, cx));
                let title = SharedString::new(t!("sidebar.direct_messages"));

                window.open_modal(cx, move |modal, _window, _cx| {
                    modal.title(title.clone()).child(compose.clone())
                })
            }),
    )
}

#[derive(Debug)]
struct Contact {
    public_key: PublicKey,
    select: bool,
}

impl AsRef<PublicKey> for Contact {
    fn as_ref(&self) -> &PublicKey {
        &self.public_key
    }
}

impl Contact {
    pub fn new(public_key: PublicKey) -> Self {
        Self {
            public_key,
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
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("compose.placeholder_npub")));

        let title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("compose.placeholder_title")));

        let error_message = cx.new(|_| None);
        let mut subscriptions = smallvec![];

        // Handle Enter event for user input
        subscriptions.push(cx.subscribe_in(
            &user_input,
            window,
            move |this, _input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.add_and_select_contact(window, cx)
                };
            },
        ));

        let get_contacts: Task<Result<Vec<Contact>, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let profiles = client.database().contacts(public_key).await?;
            let contacts = profiles
                .into_iter()
                .map(|profile| Contact::new(profile.public_key()))
                .collect_vec();

            Ok(contacts)
        });

        cx.spawn_in(window, async move |this, cx| {
            match get_contacts.await {
                Ok(contacts) => {
                    this.update(cx, |this, cx| {
                        this.extend_contacts(contacts, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
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

    async fn request_metadata(client: &Client, public_key: PublicKey) -> Result<(), Error> {
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];
        let filter = Filter::new().author(public_key).kinds(kinds).limit(10);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let public_keys: Vec<PublicKey> = self.selected(cx);

        if public_keys.is_empty() {
            self.set_error(Some(t!("compose.receiver_required").into()), cx);
            return;
        };

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

        let event: Task<Result<Room, Error>> = cx.background_spawn(async move {
            let signer = nostr_client().signer().await?;
            let public_key = signer.get_public_key().await?;

            let room = EventBuilder::private_msg_rumor(public_keys[0], "")
                .tags(Tags::from_list(tag_list))
                .build(public_key)
                .sign(&Keys::generate())
                .await
                .map(|event| Room::new(&event).kind(RoomKind::Ongoing))?;

            Ok(room)
        });

        cx.spawn_in(window, async move |this, cx| {
            match event.await {
                Ok(room) => {
                    cx.update(|window, cx| {
                        let registry = Registry::global(cx);
                        // Reset local state
                        this.update(cx, |this, cx| {
                            this.set_submitting(false, cx);
                        })
                        .ok();
                        // Create and insert the new room into the registry
                        registry.update(cx, |this, cx| {
                            this.push_room(cx.new(|_| room), cx);
                        });
                        // Close the current modal
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
            };
        })
        .detach();
    }

    fn extend_contacts<I>(&mut self, contacts: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = Contact>,
    {
        self.contacts
            .extend(contacts.into_iter().map(|contact| cx.new(|_| contact)));
        cx.notify();
    }

    fn push_contact(&mut self, contact: Contact, cx: &mut Context<Self>) {
        if !self
            .contacts
            .iter()
            .any(|e| e.read(cx).public_key == contact.public_key)
        {
            self.contacts.insert(0, cx.new(|_| contact));
            cx.notify();
        } else {
            self.set_error(Some(t!("compose.contact_existed").into()), cx);
        }
    }

    fn selected(&self, cx: &Context<Self>) -> Vec<PublicKey> {
        self.contacts
            .iter()
            .filter_map(|contact| {
                if contact.read(cx).select {
                    Some(contact.read(cx).public_key)
                } else {
                    None
                }
            })
            .collect()
    }

    fn add_and_select_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.user_input.read(cx).value().to_string();

        // Prevent multiple requests
        self.set_adding(true, cx);

        // Show loading indicator in the input
        self.user_input.update(cx, |this, cx| {
            this.set_loading(true, cx);
        });

        let task: Task<Result<Contact, Error>> = if content.contains("@") {
            cx.background_spawn(async move {
                let (tx, rx) = oneshot::channel::<Option<Nip05Profile>>();

                nostr_sdk::async_utility::task::spawn(async move {
                    let profile = nip05_profile(&content).await.ok();
                    tx.send(profile).ok();
                });

                if let Ok(Some(profile)) = rx.await {
                    let client = nostr_client();
                    let public_key = profile.public_key;
                    let contact = Contact::new(public_key).select();

                    Self::request_metadata(client, public_key).await?;

                    Ok(contact)
                } else {
                    Err(anyhow!(t!("common.not_found")))
                }
            })
        } else if let Ok(public_key) = content.to_public_key() {
            cx.background_spawn(async move {
                let client = nostr_client();
                let contact = Contact::new(public_key).select();

                Self::request_metadata(client, public_key).await?;

                Ok(contact)
            })
        } else {
            self.set_error(Some(t!("common.pubkey_invalid").into()), cx);
            return;
        };

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
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
            };
        })
        .detach();
    }

    fn set_error(&mut self, error: impl Into<Option<SharedString>>, cx: &mut Context<Self>) {
        if self.adding {
            self.set_adding(false, cx);
        }

        // Unlock the user input
        self.user_input.update(cx, |this, cx| {
            this.set_loading(false, cx);
        });

        // Update error message
        self.error_message.update(cx, |this, cx| {
            *this = error.into();
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
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let registry = Registry::read_global(cx);
        let mut items = Vec::with_capacity(self.contacts.len());

        for ix in range {
            let Some(entity) = self.contacts.get(ix).cloned() else {
                continue;
            };

            let public_key = entity.read(cx).as_ref();
            let profile = registry.get_person(public_key, cx);
            let selected = entity.read(cx).select;

            items.push(
                h_flex()
                    .id(ix)
                    .px_1()
                    .h_9()
                    .w_full()
                    .justify_between()
                    .rounded(cx.theme().radius)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .text_sm()
                            .child(Avatar::new(profile.avatar_url(proxy)).size(rems(1.75)))
                            .child(profile.display_name()),
                    )
                    .when(selected, |this| {
                        this.child(
                            Icon::new(IconName::CheckCircleFill)
                                .small()
                                .text_color(cx.theme().text_accent),
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
        let label = if self.submitting {
            t!("compose.creating_dm_button")
        } else if self.selected(cx).len() > 1 {
            t!("compose.create_group_dm_button")
        } else {
            t!("compose.create_dm_button")
        };

        let error = self.error_message.read(cx).as_ref();

        v_flex()
            .mb_3()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(SharedString::new(t!("compose.description"))),
            )
            .when_some(error, |this, msg| {
                this.child(
                    div()
                        .italic()
                        .text_sm()
                        .text_color(cx.theme().danger_foreground)
                        .child(msg.clone()),
                )
            })
            .child(
                h_flex()
                    .gap_1()
                    .h_10()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(SharedString::new(t!("compose.subject_label"))),
                    )
                    .child(TextInput::new(&self.title_input).small().appearance(false)),
            )
            .child(
                v_flex()
                    .mt_1()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(SharedString::new(t!("compose.to_label"))),
                            )
                            .child(
                                h_flex()
                                    .px_1()
                                    .gap_1()
                                    .child(
                                        TextInput::new(&self.user_input)
                                            .small()
                                            .disabled(self.adding),
                                    )
                                    .child(
                                        Button::new("add")
                                            .icon(IconName::PlusCircleFill)
                                            .ghost()
                                            .loading(self.adding)
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
                                            .child(SharedString::new(t!(
                                                "compose.no_contacts_message"
                                            ))),
                                    )
                                    .child(
                                        div().text_xs().text_color(cx.theme().text_muted).child(
                                            SharedString::new(t!(
                                                "compose.no_contacts_description"
                                            )),
                                        ),
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
                                .min_h(px(280.)),
                            )
                        }
                    }),
            )
            .child(
                Button::new("create_dm_btn")
                    .label(label)
                    .primary()
                    .small()
                    .w_full()
                    .loading(self.submitting)
                    .disabled(self.submitting || self.adding)
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.submit(window, cx);
                    })),
            )
    }
}
