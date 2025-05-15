use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Error};
use async_utility::task::spawn;
use chats::{
    message::{Message, RoomMessage},
    room::Room,
    ChatRegistry,
};
use common::{nip96_upload, profile::SharedProfile};
use global::{constants::IMAGE_SERVICE, get_client};
use gpui::{
    div, img, impl_internal_actions, list, prelude::FluentBuilder, px, red, relative, svg, white,
    AnyElement, App, AppContext, Context, Element, Empty, Entity, EventEmitter, Flatten,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ListAlignment, ListState, ObjectFit,
    ParentElement, PathPromptOptions, Render, SharedString, StatefulInteractiveElement, Styled,
    StyledImage, Subscription, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};
use smol::fs;
use theme::ActiveTheme;
use ui::{
    button::{Button, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    emoji_picker::EmojiPicker,
    input::{InputEvent, TextInput},
    notification::Notification,
    popup_menu::PopupMenu,
    text::RichText,
    v_flex, ContextModal, Disableable, Icon, IconName, Size, StyledExt,
};

use crate::views::subject;

const ALERT: &str = "has not set up Messaging (DM) Relays, so they will NOT receive your messages.";
const DESC: &str = "This conversation is private. Only members can see each other's messages.";

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ChangeSubject(pub String);

impl_internal_actions!(chat, [ChangeSubject]);

pub fn init(id: &u64, window: &mut Window, cx: &mut App) -> Result<Arc<Entity<Chat>>, Error> {
    if let Some(room) = ChatRegistry::global(cx).read(cx).room(id, cx) {
        Ok(Arc::new(Chat::new(id, room, window, cx)))
    } else {
        Err(anyhow!("Chat Room not found."))
    }
}

pub struct Chat {
    // Panel
    id: SharedString,
    focus_handle: FocusHandle,
    // Chat Room
    room: Entity<Room>,
    messages: Entity<Vec<RoomMessage>>,
    text_data: HashMap<EventId, RichText>,
    list_state: ListState,
    // New Message
    input: Entity<TextInput>,
    // Media Attachment
    attaches: Entity<Option<Vec<Url>>>,
    uploading: bool,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 2]>,
}

impl Chat {
    pub fn new(id: &u64, room: Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let messages = cx.new(|_| vec![RoomMessage::announcement()]);
        let attaches = cx.new(|_| None);
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .text_size(Size::Small)
                .placeholder("Message...")
        });

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, input, event, window, cx| {
                    if let InputEvent::PressEnter = event {
                        if input.read(cx).text().is_empty() {
                            window.push_notification("Cannot send an empty message", cx);
                        } else {
                            this.send_message(window, cx);
                        }
                    }
                },
            ));

            subscriptions.push(
                cx.subscribe_in(&room, window, move |this, _, inc, _window, cx| {
                    let old_len = this.messages.read(cx).len();
                    let message = RoomMessage::user(inc.event.clone());

                    cx.update_entity(&this.messages, |this, cx| {
                        this.extend(vec![message]);
                        cx.notify();
                    });

                    this.list_state.splice(old_len..old_len, 1);
                }),
            );

            // Initialize list state
            // [item_count] always equal to 1 at the beginning
            let list_state = ListState::new(1, ListAlignment::Bottom, px(1024.), {
                let this = cx.entity().downgrade();
                move |ix, window, cx| {
                    this.update(cx, |this, cx| {
                        this.render_message(ix, window, cx).into_any_element()
                    })
                    .unwrap_or(Empty.into_any())
                }
            });

            let this = Self {
                focus_handle: cx.focus_handle(),
                uploading: false,
                id: id.to_string().into(),
                text_data: HashMap::new(),
                room,
                messages,
                list_state,
                input,
                attaches,
                subscriptions,
            };

            // Verify messaging relays of all members
            this.verify_messaging_relays(window, cx);

            // Load all messages from database
            this.load_messages(window, cx);

            this
        })
    }

    fn verify_messaging_relays(&self, window: &mut Window, cx: &mut Context<Self>) {
        let room = self.room.read(cx);
        let task = room.messaging_relays(cx);

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(result) = task.await {
                this.update(cx, |this, cx| {
                    result.into_iter().for_each(|item| {
                        if !item.1 {
                            let profile = this
                                .room
                                .read_with(cx, |this, _| this.profile_by_pubkey(&item.0, cx));

                            this.push_system_message(
                                format!("{} {}", profile.shared_name(), ALERT),
                                cx,
                            );
                        }
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    /// Load all messages belonging to this room
    fn load_messages(&self, window: &mut Window, cx: &mut Context<Self>) {
        let room = self.room.read(cx);
        let task = room.load_messages(cx);

        cx.spawn_in(window, async move |this, cx| {
            match task.await {
                Ok(events) => {
                    this.update(cx, |this, cx| {
                        let old_len = this.messages.read(cx).len();
                        let new_len = events.len();

                        // Extend the messages list with the new events
                        this.messages.update(cx, |this, cx| {
                            this.extend(events);
                            cx.notify();
                        });

                        // Update list state with the new messages
                        this.list_state.splice(old_len..old_len, new_len);

                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Get user input message including all attachments
    fn message(&self, cx: &Context<Self>) -> String {
        let mut content = self.input.read(cx).text().to_string();

        // Get all attaches and merge its with message
        if let Some(attaches) = self.attaches.read(cx).as_ref() {
            if !attaches.is_empty() {
                content = format!(
                    "{}\n{}",
                    content,
                    attaches
                        .iter()
                        .map(|url| url.to_string())
                        .collect_vec()
                        .join("\n")
                )
            }
        }

        content
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |this, cx| {
            this.set_loading(true, cx);
            this.set_disabled(true, cx);
        });

        let content = self.message(cx);
        let room = self.room.read(cx);

        if let Some(message) = room.create_temp_message(&content, cx) {
            let task = room.send_message_in_background(&content, message.created_at, cx);

            // Optimistically update message list
            self.push_user_message(message, cx);

            // Reset the input state
            self.input.update(cx, |this, cx| {
                this.set_text("", window, cx);
                this.set_loading(false, cx);
                this.set_disabled(false, cx);
            });

            // Continue sending the message in the background
            if let Some(send_message) = task {
                send_message.detach_and_log_err(cx);
            }
        }
    }

    fn push_user_message(&self, message: Message, cx: &mut Context<Self>) {
        let old_len = self.messages.read(cx).len();
        let message = RoomMessage::user(message);

        cx.update_entity(&self.messages, |this, cx| {
            this.extend(vec![message]);
            cx.notify();
        });

        self.list_state.splice(old_len..old_len, 1);
    }

    fn push_system_message(&self, content: String, cx: &mut Context<Self>) {
        let old_len = self.messages.read(cx).len();
        let message = RoomMessage::system(content.into());

        cx.update_entity(&self.messages, |this, cx| {
            this.extend(vec![message]);
            cx.notify();
        });

        self.list_state.splice(old_len..old_len, 1);
    }

    fn upload_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uploading {
            return;
        }

        self.uploading(true, cx);

        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    let Some(path) = paths.pop() else {
                        return;
                    };

                    if let Ok(file_data) = fs::read(path).await {
                        let client = get_client();
                        let (tx, rx) = oneshot::channel::<Url>();

                        // spawn task via async_utility
                        spawn(async move {
                            if let Ok(url) = nip96_upload(client, file_data).await {
                                _ = tx.send(url);
                            }
                        });

                        if let Ok(url) = rx.await {
                            this.update(cx, |this, cx| {
                                this.uploading(false, cx);
                                this.attaches.update(cx, |this, cx| {
                                    if let Some(model) = this.as_mut() {
                                        model.push(url);
                                    } else {
                                        *this = Some(vec![url]);
                                    }
                                    cx.notify();
                                });
                            })
                            .ok();
                        }
                    }
                }
                Ok(None) => {
                    this.update(cx, |this, cx| {
                        this.uploading(false, cx);
                    })
                    .ok();
                }
                Err(_) => {}
            }
        })
        .detach();
    }

    fn remove_media(&mut self, url: &Url, _window: &mut Window, cx: &mut Context<Self>) {
        self.attaches.update(cx, |model, cx| {
            if let Some(urls) = model.as_mut() {
                if let Some(ix) = urls.iter().position(|x| x == url) {
                    urls.remove(ix);
                    cx.notify();
                }
            }
        });
    }

    fn uploading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.uploading = status;
        cx.notify();
    }

    fn render_message(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(message) = self.messages.read(cx).get(ix) else {
            return div().into_element();
        };

        let text_data = &mut self.text_data;

        div()
            .group("")
            .w_full()
            .relative()
            .flex()
            .gap_3()
            .px_3()
            .py_2()
            .map(|this| match message {
                RoomMessage::User(item) => {
                    let text = text_data
                        .entry(item.id)
                        .or_insert_with(|| RichText::new(item.content.to_owned(), &item.mentions));

                    this.hover(|this| this.bg(cx.theme().surface_background))
                        .text_sm()
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .w(px(2.))
                                .h_full()
                                .bg(cx.theme().border_transparent)
                                .group_hover("", |this| this.bg(cx.theme().element_active)),
                        )
                        .child(img(item.author.shared_avatar()).size_8().flex_shrink_0())
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_initial()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .flex()
                                        .items_baseline()
                                        .gap_2()
                                        .child(
                                            div().font_semibold().child(item.author.shared_name()),
                                        )
                                        .child(
                                            div()
                                                .text_color(cx.theme().text_placeholder)
                                                .child(item.ago()),
                                        ),
                                )
                                .child(text.element("body".into(), window, cx)),
                        )
                }
                RoomMessage::System(content) => this
                    .items_center()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .w(px(2.))
                            .h_full()
                            .bg(cx.theme().border_transparent)
                            .group_hover("", |this| this.bg(red())),
                    )
                    .child(img("brand/avatar.png").size_8().flex_shrink_0())
                    .text_sm()
                    .text_color(red())
                    .child(content.clone()),
                RoomMessage::Announcement => this
                    .w_full()
                    .h_32()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .text_xs()
                    .text_color(cx.theme().text_placeholder)
                    .line_height(relative(1.3))
                    .child(
                        svg()
                            .path("brand/coop.svg")
                            .size_10()
                            .text_color(cx.theme().elevated_surface_background),
                    )
                    .child(DESC),
            })
    }
}

impl Panel for Chat {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.room.read_with(cx, |this, _| {
            let label = this.display_name(cx);
            let url = this.display_image(cx);

            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(img(url).size_5().flex_shrink_0())
                .child(label)
                .into_any()
        })
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, cx: &App) -> Vec<Button> {
        let id = self.room.read(cx).id;
        let subject = self
            .room
            .read(cx)
            .subject
            .as_ref()
            .map(|subject| subject.to_string());

        let button = Button::new("subject")
            .icon(IconName::EditFill)
            .tooltip("Change Subject")
            .on_click(move |_, window, cx| {
                let subject = subject::init(id, subject.clone(), window, cx);

                window.open_modal(cx, move |this, _window, _cx| {
                    this.title("Change the subject of the conversation")
                        .child(subject.clone())
                });
            });

        vec![button]
    }
}

impl EventEmitter<PanelEvent> for Chat {}

impl Focusable for Chat {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Chat {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(list(self.list_state.clone()).flex_1())
            .child(
                div().flex_shrink_0().px_3().py_2().child(
                    div()
                        .flex()
                        .flex_col()
                        .when_some(self.attaches.read(cx).as_ref(), |this, attaches| {
                            this.gap_1p5().children(attaches.iter().map(|url| {
                                let url = url.clone();
                                let path: SharedString = url.to_string().into();

                                div()
                                    .id(path.clone())
                                    .relative()
                                    .w_16()
                                    .child(
                                        img(format!(
                                            "{}/?url={}&w=128&h=128&fit=cover&n=-1",
                                            IMAGE_SERVICE, path
                                        ))
                                        .size_16()
                                        .shadow_lg()
                                        .rounded(cx.theme().radius)
                                        .object_fit(ObjectFit::ScaleDown),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .top_neg_2()
                                            .right_neg_2()
                                            .size_4()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(red())
                                            .child(
                                                Icon::new(IconName::Close)
                                                    .size_2()
                                                    .text_color(white()),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.remove_media(&url, window, cx);
                                    }))
                            }))
                        })
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2p5()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(cx.theme().text_muted)
                                        .child(
                                            Button::new("upload")
                                                .icon(Icon::new(IconName::Upload))
                                                .ghost()
                                                .disabled(self.uploading)
                                                .loading(self.uploading)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.upload_media(window, cx);
                                                    },
                                                )),
                                        )
                                        .child(
                                            EmojiPicker::new(self.input.downgrade())
                                                .icon(IconName::EmojiFill),
                                        ),
                                )
                                .child(self.input.clone()),
                        ),
                ),
            )
    }
}
