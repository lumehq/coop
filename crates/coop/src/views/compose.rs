use std::ops::Range;
use std::time::Duration;

use account::Account;
use anyhow::{anyhow, Error};
use chat::{ChatRegistry, Room};
use common::{nip05_profile, RenderedProfile, TextUtils, BOOTSTRAP_RELAYS};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, rems, uniform_list, App, AppContext, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, RetainAllImageCache, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, Window,
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
                        SharedString::from("Create Group DM")
                    } else {
                        SharedString::from("Create DM")
                    };

                    modal
                        .alert()
                        .overlay_closable(true)
                        .keyboard(true)
                        .show_close(true)
                        .button_props(ModalButtonProps::default().ok_text(label))
                        .title(SharedString::from("Direct Messages"))
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Contact {
    public_key: PublicKey,
    selected: bool,
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
            selected: false,
        }
    }

    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }
}

pub struct Compose {
    /// Input for the room's subject
    title_input: Entity<InputState>,

    /// Input for the room's members
    user_input: Entity<InputState>,

    /// User's contacts
    contacts: Entity<Vec<Contact>>,

    /// Error message
    error_message: Entity<Option<SharedString>>,

    image_cache: Entity<RetainAllImageCache>,
    _subscriptions: SmallVec<[Subscription; 2]>,
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let contacts = cx.new(|_| vec![]);
        let error_message = cx.new(|_| None);

        let user_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("npub or nprofile..."));

        let title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Family...(Optional)"));

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        let get_contacts: Task<Result<Vec<Contact>, Error>> = cx.background_spawn(async move {
            let client = client();
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
            // Clear the image cache when sidebar is closed
            cx.on_release_in(window, move |this, window, cx| {
                this.image_cache.update(cx, |this, cx| {
                    this.clear(window, cx);
                })
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
            contacts,
            image_cache: RetainAllImageCache::new(cx),
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    async fn request_metadata(public_key: PublicKey) -> Result<(), Error> {
        let client = client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);
        let kinds = vec![Kind::Metadata, Kind::ContactList];
        let filter = Filter::new().author(public_key).kinds(kinds).limit(10);

        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter, Some(opts))
            .await?;

        Ok(())
    }

    fn extend_contacts<I>(&mut self, contacts: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = Contact>,
    {
        self.contacts.update(cx, |this, cx| {
            this.extend(contacts);
            cx.notify();
        });
    }

    fn push_contact(&mut self, contact: Contact, window: &mut Window, cx: &mut Context<Self>) {
        let pk = contact.public_key;

        if !self.contacts.read(cx).iter().any(|c| c.public_key == pk) {
            self._tasks.push(cx.background_spawn(async move {
                Self::request_metadata(pk).await.ok();
            }));

            cx.defer_in(window, |this, window, cx| {
                this.contacts.update(cx, |this, cx| {
                    this.insert(0, contact);
                    cx.notify();
                });
                this.user_input.update(cx, |this, cx| {
                    this.set_value("", window, cx);
                    this.set_loading(false, cx);
                });
            });
        } else {
            self.set_error("Contact already added", cx);
        }
    }

    fn select_contact(&mut self, public_key: PublicKey, cx: &mut Context<Self>) {
        self.contacts.update(cx, |this, cx| {
            if let Some(contact) = this.iter_mut().find(|c| c.public_key == public_key) {
                contact.selected = true;
            }
            cx.notify();
        });
    }

    fn add_and_select_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.user_input.read(cx).value().to_string();

        // Show loading indicator in the input
        self.user_input.update(cx, |this, cx| {
            this.set_loading(true, cx);
        });

        if let Ok(public_key) = content.to_public_key() {
            let contact = Contact::new(public_key).selected();
            self.push_contact(contact, window, cx);
        } else if content.contains("@") {
            let task = Tokio::spawn(cx, async move {
                if let Ok(profile) = nip05_profile(&content).await {
                    let public_key = profile.public_key;
                    let contact = Contact::new(public_key).selected();

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
                            this.set_error(e.to_string(), cx);
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

    fn selected(&self, cx: &App) -> Vec<PublicKey> {
        self.contacts
            .read(cx)
            .iter()
            .filter_map(|contact| {
                if contact.selected {
                    Some(contact.public_key)
                } else {
                    None
                }
            })
            .collect()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat = ChatRegistry::global(cx);

        let account = Account::global(cx);
        let public_key = account.read(cx).public_key();

        let receivers: Vec<PublicKey> = self.selected(cx);
        let subject_input = self.title_input.read(cx).value();
        let subject = (!subject_input.is_empty()).then(|| subject_input.to_string());

        if !self.user_input.read(cx).value().is_empty() {
            self.add_and_select_contact(window, cx);
            return;
        };

        chat.update(cx, |this, cx| {
            this.push_room(cx.new(|_| Room::new(subject, public_key, receivers)), cx);
        });

        window.close_modal(cx);
    }

    fn set_error(&mut self, error: impl Into<SharedString>, cx: &mut Context<Self>) {
        // Unlock the user input
        self.user_input.update(cx, |this, cx| {
            this.set_loading(false, cx);
        });

        // Update error message
        self.error_message.update(cx, |this, cx| {
            *this = Some(error.into());
            cx.notify();
        });

        // Dismiss error after 2 seconds
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;

            this.update(cx, |this, cx| {
                this.error_message.update(cx, |this, cx| {
                    *this = None;
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    fn list_items(&self, range: Range<usize>, cx: &Context<Self>) -> Vec<impl IntoElement> {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let persons = PersonRegistry::global(cx);
        let mut items = Vec::with_capacity(self.contacts.read(cx).len());

        for ix in range {
            let Some(contact) = self.contacts.read(cx).get(ix) else {
                continue;
            };

            let public_key = contact.public_key;
            let profile = persons.read(cx).get(&public_key, cx);

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
                            .child(Avatar::new(profile.avatar(proxy)).size(rems(1.75)))
                            .child(profile.display_name()),
                    )
                    .when(contact.selected, |this| {
                        this.child(
                            Icon::new(IconName::CheckCircleFill)
                                .small()
                                .text_color(cx.theme().text_accent),
                        )
                    })
                    .hover(|this| this.bg(cx.theme().elevated_surface_background))
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.select_contact(public_key, cx);
                    })),
            );
        }

        items
    }
}

impl Render for Compose {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.error_message.read(cx).as_ref();
        let loading = self.user_input.read(cx).loading;
        let contacts = self.contacts.read(cx);

        v_flex()
            .image_cache(self.image_cache.clone())
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .child(SharedString::from("Start a conversation with someone using their npub or NIP-05 (like foo@bar.com).")),
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
                            .child(SharedString::from("Subject:")),
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
                                    .child(SharedString::from("To:")),
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
                        if contacts.is_empty() {
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
                                            .child(SharedString::from("No contacts")),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_muted)
                                            .child(SharedString::from("Your recently contacts will appear here.")),
                                    ),
                            )
                        } else {
                            this.child(
                                uniform_list(
                                    "contacts",
                                    contacts.len(),
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
