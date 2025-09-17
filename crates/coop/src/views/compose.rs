use std::ops::Range;
use std::time::Duration;

use anyhow::{anyhow, Error};
use common::display::{ReadableProfile, TextUtils};
use common::nip05::nip05_profile;
use global::constants::BOOTSTRAP_RELAYS;
use global::{css, nostr_client};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, App, AppContext, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use nostr_sdk::prelude::*;
use registry::room::Room;
use registry::Registry;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::Timer;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::input::{InputEvent, InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::notification::Notification;
use ui::{h_flex, v_flex, ContextModal, Disableable, Icon, IconName, Sizable, StyledExt};

pub fn compose_button() -> impl IntoElement {
    div().child(
        Button::new("compose")
            .icon(IconName::Plus)
            .ghost_alt()
            .cta()
            .small()
            .rounded()
            .on_click(move |_, window, cx| {
                let compose = cx.new(|cx| Compose::new(window, cx));
                let weak_view = compose.downgrade();

                window.open_modal(cx, move |modal, _window, cx| {
                    let weak_view = weak_view.clone();
                    let label = if compose.read(cx).selected(cx).len() > 1 {
                        shared_t!("compose.create_group_dm_button")
                    } else {
                        shared_t!("compose.create_dm_button")
                    };

                    modal
                        .alert()
                        .overlay_closable(true)
                        .keyboard(true)
                        .show_close(true)
                        .button_props(ModalButtonProps::default().ok_text(label))
                        .title(shared_t!("sidebar.direct_messages"))
                        .child(compose.clone())
                        .on_ok(move |_, window, cx| {
                            weak_view
                                .update(cx, |this, cx| {
                                    this.submit(window, cx);
                                })
                                .ok();

                            // false to prevent the modal from closing
                            false
                        })
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

    _subscriptions: SmallVec<[Subscription; 1]>,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let error_message = cx.new(|_| None);

        let user_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("npub or nprofile..."));

        let title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Family...(Optional)"));

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        let get_contacts: Task<Result<Vec<Contact>, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let signer = client.signer().await?;
            let public_key = signer.get_public_key().await?;
            let profiles = client.database().contacts(public_key).await?;
            let contacts: Vec<Contact> = profiles
                .into_iter()
                .map(|profile| Contact::new(profile.public_key()))
                .collect();

            Ok(contacts)
        });

        tasks.push(
            // Load all contacts
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
            }),
        );

        subscriptions.push(
            // Handle Enter event for user input
            cx.subscribe_in(
                &user_input,
                window,
                move |this, _input, event, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.add_and_select_contact(window, cx)
                    };
                },
            ),
        );

        Self {
            title_input,
            user_input,
            error_message,
            contacts: vec![],
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn request_metadata(public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let css = css();
        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];
        let filter = Filter::new().author(public_key).kinds(kinds).limit(10);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, css.auto_close_opts)
            .await?;

        Ok(())
    }

    fn extend_contacts<I>(&mut self, contacts: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = Contact>,
    {
        self.contacts
            .extend(contacts.into_iter().map(|contact| cx.new(|_| contact)));
        cx.notify();
    }

    fn push_contact(&mut self, contact: Contact, window: &mut Window, cx: &mut Context<Self>) {
        let pk = contact.public_key;

        if !self.contacts.iter().any(|e| e.read(cx).public_key == pk) {
            cx.background_spawn(async move {
                Self::request_metadata(pk).await.ok();
            })
            .detach();

            cx.defer_in(window, |this, window, cx| {
                this.contacts.insert(0, cx.new(|_| contact));
                this.user_input.update(cx, |this, cx| {
                    this.set_value("", window, cx);
                    this.set_loading(false, cx);
                });

                cx.notify();
            });
        } else {
            self.set_error(Some(t!("compose.contact_existed").into()), cx);
        }
    }

    fn selected(&self, cx: &App) -> Vec<PublicKey> {
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

        // Show loading indicator in the input
        self.user_input.update(cx, |this, cx| {
            this.set_loading(true, cx);
        });

        if let Ok(public_key) = content.to_public_key() {
            let contact = Contact::new(public_key).select();
            self.push_contact(contact, window, cx);
        } else if content.contains("@") {
            let task = Tokio::spawn(cx, async move {
                if let Ok(profile) = nip05_profile(&content).await {
                    let public_key = profile.public_key;
                    let contact = Contact::new(public_key).select();

                    Ok(contact)
                } else {
                    Err(anyhow!("Not found"))
                }
            });

            cx.spawn_in(window, async move |this, cx| {
                match task.await {
                    Ok(Ok(contact)) => {
                        this.update_in(cx, |this, window, cx| {
                            this.push_contact(contact, window, cx);
                        })
                        .ok();
                    }
                    Ok(Err(e)) => {
                        this.update(cx, |this, cx| {
                            this.set_error(Some(e.to_string().into()), cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        log::error!("Tokio error: {e}");
                    }
                };
            })
            .detach();
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let registry = Registry::global(cx);
        let public_keys: Vec<PublicKey> = self.selected(cx);

        if !self.user_input.read(cx).value().is_empty() {
            self.add_and_select_contact(window, cx);
            return;
        };

        if public_keys.is_empty() {
            self.set_error(Some(t!("compose.receiver_required").into()), cx);
            return;
        };

        // Convert selected pubkeys into Nostr tags
        let mut tags: Tags = Tags::from_list(
            public_keys
                .iter()
                .map(|pubkey| Tag::public_key(pubkey.to_owned()))
                .collect(),
        );

        // Add subject if it is present
        if !self.title_input.read(cx).value().is_empty() {
            tags.push(Tag::custom(
                TagKind::Subject,
                vec![self.title_input.read(cx).value().to_string()],
            ));
        }

        // Create a new room
        let room = Room::new(public_keys[0], tags, cx);

        // Insert the new room into the registry
        registry.update(cx, |this, cx| {
            this.push_room(cx.new(|_| room), cx);
        });

        // Close the current modal
        window.close_modal(cx);
    }

    fn set_error(&mut self, error: impl Into<Option<SharedString>>, cx: &mut Context<Self>) {
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
                    .px_2()
                    .h_11()
                    .w_full()
                    .justify_between()
                    .rounded(cx.theme().radius)
                    .child(
                        h_flex()
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
        let error = self.error_message.read(cx).as_ref();
        let loading = self.user_input.read(cx).loading(cx);

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(shared_t!("compose.description")),
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
                            .child(shared_t!("compose.subject_label")),
                    )
                    .child(TextInput::new(&self.title_input).small().appearance(false)),
            )
            .child(
                v_flex()
                    .pt_1()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(shared_t!("compose.to_label")),
                            )
                            .child(
                                TextInput::new(&self.user_input)
                                    .small()
                                    .disabled(loading)
                                    .suffix(
                                        Button::new("add")
                                            .icon(IconName::PlusCircleFill)
                                            .transparent()
                                            .small()
                                            .disabled(loading)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.add_and_select_contact(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .map(|this| {
                        if self.contacts.is_empty() {
                            this.child(
                                v_flex()
                                    .h_24()
                                    .w_full()
                                    .items_center()
                                    .justify_center()
                                    .text_center()
                                    .text_xs()
                                    .child(
                                        div()
                                            .font_semibold()
                                            .line_height(relative(1.2))
                                            .child(shared_t!("compose.no_contacts_message")),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_muted)
                                            .child(shared_t!("compose.no_contacts_description")),
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
                                .h(px(300.)),
                            )
                        }
                    }),
            )
    }
}
