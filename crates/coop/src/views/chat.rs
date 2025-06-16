use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use async_utility::task::spawn;
use chats::message::Message;
use chats::room::{Room, RoomKind, SendError};
use common::nip96_upload;
use common::profile::RenderProfile;
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, impl_internal_actions, list, px, red, relative, rems, svg, white, AnyElement, App,
    AppContext, ClipboardItem, Context, Div, Element, Empty, Entity, EventEmitter, Flatten,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ListAlignment, ListState, ObjectFit,
    ParentElement, PathPromptOptions, Render, RetainAllImageCache, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, Subscription, Window,
};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::fs;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::emoji_picker::EmojiPicker;
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::text::RichText;
use ui::{
    v_flex, ContextModal, Disableable, Icon, IconName, InteractiveElementExt, Sizable, StyledExt,
};

use crate::views::subject;

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ChangeSubject(pub String);

impl_internal_actions!(chat, [ChangeSubject]);

pub fn init(room: Entity<Room>, window: &mut Window, cx: &mut App) -> Arc<Entity<Chat>> {
    Arc::new(Chat::new(room, window, cx))
}

pub struct Chat {
    // Panel
    id: SharedString,
    focus_handle: FocusHandle,
    // Chat Room
    room: Entity<Room>,
    messages: Entity<Vec<Rc<RefCell<Message>>>>,
    text_data: HashMap<EventId, RichText>,
    list_state: ListState,
    // New Message
    input: Entity<InputState>,
    replies_to: Entity<Option<Vec<Message>>>,
    // Media Attachment
    attaches: Entity<Option<Vec<Url>>>,
    uploading: bool,
    image_cache: Entity<RetainAllImageCache>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 2]>,
}

impl Chat {
    pub fn new(room: Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let attaches = cx.new(|_| None);
        let replies_to = cx.new(|_| None);

        let messages = cx.new(|_| {
            let message = Message::builder()
                .content(
                    "This conversation is private. Only members can see each other's messages."
                        .into(),
                )
                .build_rc()
                .unwrap();

            vec![message]
        });

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Message...")
                .multi_line()
                .prevent_new_line_on_enter()
                .rows(1)
                .multi_line()
                .auto_grow(1, 20)
                .clean_on_escape()
        });

        cx.new(|cx| {
            let mut subscriptions = smallvec![];

            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, input, event, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        if input.read(cx).value().trim().is_empty() {
                            window.push_notification("Cannot send an empty message", cx);
                        } else {
                            this.send_message(window, cx);
                        }
                    }
                },
            ));

            subscriptions.push(
                cx.subscribe_in(&room, window, move |this, _, incoming, _w, cx| {
                    // Check if the incoming message is the same as the new message created by optimistic update
                    if this.prevent_duplicate_message(&incoming.0, cx) {
                        return;
                    }

                    let old_len = this.messages.read(cx).len();
                    let message = incoming.0.clone().into_rc();

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

            Self {
                image_cache: RetainAllImageCache::new(cx),
                focus_handle: cx.focus_handle(),
                uploading: false,
                id: room.read(cx).id.to_string().into(),
                text_data: HashMap::new(),
                room,
                messages,
                list_state,
                input,
                replies_to,
                attaches,
                subscriptions,
            }
        })
    }

    /// Load all messages belonging to this room
    pub(crate) fn load_messages(&self, window: &mut Window, cx: &mut Context<Self>) {
        let room = self.room.read(cx);
        let load_messages = room.load_messages(cx);

        cx.spawn_in(window, async move |this, cx| {
            match load_messages.await {
                Ok(messages) => {
                    this.update(cx, |this, cx| {
                        let old_len = this.messages.read(cx).len();
                        let new_len = messages.len();

                        // Extend the messages list with the new events
                        this.messages.update(cx, |this, cx| {
                            this.extend(messages.into_iter().map(|e| e.into_rc()));
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
        let mut content = self.input.read(cx).value().trim().to_string();

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

    // TODO: find a better way to prevent duplicate messages during optimistic updates
    fn prevent_duplicate_message(&self, new_msg: &Message, cx: &Context<Self>) -> bool {
        let Some(account) = Identity::get_global(cx).profile(cx) else {
            return false;
        };

        let Some(author) = new_msg.author.as_ref() else {
            return false;
        };

        if account.public_key() != author.public_key() {
            return false;
        }

        let min_timestamp = new_msg.created_at.as_u64().saturating_sub(10);

        self.messages
            .read(cx)
            .iter()
            .filter(|m| {
                m.borrow()
                    .author
                    .as_ref()
                    .is_some_and(|p| p.public_key() == account.public_key())
            })
            .any(|existing| {
                let existing = existing.borrow();
                // Check if messages are within the time window
                (existing.created_at.as_u64() >= min_timestamp) &&
		        // Compare content and author
		        (existing.content == new_msg.content) &&
		        (existing.author == new_msg.author)
            })
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |this, cx| {
            this.set_loading(true, cx);
            this.set_disabled(true, cx);
        });

        // Get the message which includes all attachments
        let content = self.message(cx);
        // Get replies_to if it's present
        let replies = self.replies_to.read(cx).as_ref();
        // Get the current room entity
        let room = self.room.read(cx);
        // Create a temporary message for optimistic update
        let temp_message = room.create_temp_message(&content, replies, cx);
        // Create a task for sending the message in the background
        let send_message = room.send_in_background(&content, replies, cx);

        if let Some(message) = temp_message {
            let id = message.id;
            // Optimistically update message list
            self.insert_message(message, cx);
            // Remove all replies
            self.remove_all_replies(cx);

            // Reset the input state
            self.input.update(cx, |this, cx| {
                this.set_loading(false, cx);
                this.set_disabled(false, cx);
                this.set_value("", window, cx);
            });

            // Continue sending the message in the background
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(reports) = send_message.await {
                    if !reports.is_empty() {
                        this.update(cx, |this, cx| {
                            this.room.update(cx, |this, cx| {
                                if this.kind != RoomKind::Ongoing {
                                    this.kind = RoomKind::Ongoing;
                                    cx.notify();
                                }
                            });

                            this.messages.update(cx, |this, cx| {
                                if let Some(msg) = id.and_then(|id| {
                                    this.iter().find(|msg| msg.borrow().id == Some(id)).cloned()
                                }) {
                                    msg.borrow_mut().errors = Some(reports);
                                    cx.notify();
                                }
                            });
                        })
                        .ok();
                    }
                }
            })
            .detach();
        }
    }

    fn insert_message(&self, message: Message, cx: &mut Context<Self>) {
        let old_len = self.messages.read(cx).len();
        let message = message.into_rc();

        cx.update_entity(&self.messages, |this, cx| {
            this.extend(vec![message]);
            cx.notify();
        });

        self.list_state.splice(old_len..old_len, 1);
    }

    fn scroll_to(&self, id: EventId, cx: &Context<Self>) {
        if let Some(ix) = self
            .messages
            .read(cx)
            .iter()
            .position(|m| m.borrow().id == Some(id))
        {
            self.list_state.scroll_to_reveal_item(ix);
        }
    }

    fn reply(&mut self, message: Message, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            if let Some(replies) = this {
                replies.push(message);
            } else {
                *this = Some(vec![message])
            }
            cx.notify();
        });
    }

    fn remove_reply(&mut self, id: EventId, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            if let Some(replies) = this {
                if let Some(ix) = replies.iter().position(|m| m.id == Some(id)) {
                    replies.remove(ix);
                    cx.notify();
                }
            }
        });
    }

    fn remove_all_replies(&mut self, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            *this = None;
            cx.notify();
        });
    }

    fn upload_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uploading {
            return;
        }

        self.uploading(true, cx);

        let nip96 = AppSettings::get_global(cx).settings.media_server.clone();
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
                        let (tx, rx) = oneshot::channel::<Option<Url>>();

                        // Spawn task via async utility instead of GPUI context
                        spawn(async move {
                            let url = match nip96_upload(&shared_state().client, nip96, file_data)
                                .await
                            {
                                Ok(url) => Some(url),
                                Err(e) => {
                                    log::error!("Upload error: {e}");
                                    None
                                }
                            };

                            _ = tx.send(url);
                        });

                        if let Ok(Some(url)) = rx.await {
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
                        } else {
                            this.update(cx, |this, cx| {
                                this.uploading(false, cx);
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
                Err(e) => {
                    log::error!("System error: {e}")
                }
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

    fn render_attach(&mut self, url: &Url, cx: &Context<Self>) -> impl IntoElement {
        let url = url.clone();
        let path: SharedString = url.to_string().into();

        div()
            .id("")
            .relative()
            .w_16()
            .child(
                img(path.clone())
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
                    .child(Icon::new(IconName::Close).size_2().text_color(white())),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.remove_media(&url, window, cx);
            }))
    }

    fn render_reply(&mut self, message: &Message, cx: &Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .pl_2()
            .border_l_2()
            .border_color(cx.theme().element_active)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_1()
                            .text_xs()
                            .text_color(cx.theme().text_muted)
                            .child("Replying to:")
                            .child(
                                div()
                                    .text_color(cx.theme().text_accent)
                                    .child(message.author.as_ref().unwrap().render_name()),
                            ),
                    )
                    .child(
                        Button::new("remove-reply")
                            .icon(IconName::Close)
                            .xsmall()
                            .ghost()
                            .on_click({
                                let id = message.id.unwrap();
                                cx.listener(move |this, _, _, cx| {
                                    this.remove_reply(id, cx);
                                })
                            }),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .text_ellipsis()
                    .line_clamp(1)
                    .child(message.content.clone()),
            )
    }

    fn render_message(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(message) = self.messages.read(cx).get(ix) else {
            return div().id(ix);
        };

        let proxy = AppSettings::get_global(cx).settings.proxy_user_avatars;
        let hide_avatar = AppSettings::get_global(cx).settings.hide_user_avatars;

        let message = message.borrow();

        // Message without ID, Author probably the placeholder
        let (Some(id), Some(author)) = (message.id, message.author.as_ref()) else {
            return div()
                .id(ix)
                .group("")
                .w_full()
                .relative()
                .flex()
                .gap_3()
                .px_3()
                .py_2()
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
                .child(message.content.clone());
        };

        let texts = self
            .text_data
            .entry(id)
            .or_insert_with(|| RichText::new(message.content.to_string(), &message.mentions));

        div()
            .id(ix)
            .group("")
            .relative()
            .w_full()
            .py_1()
            .px_3()
            .child(
                div()
                    .flex()
                    .gap_3()
                    .when(!hide_avatar, |this| {
                        this.child(Avatar::new(author.render_avatar(proxy)).size(rems(2.)))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .flex_initial()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap_2()
                                    .text_sm()
                                    .child(
                                        div()
                                            .font_semibold()
                                            .text_color(cx.theme().text)
                                            .child(author.render_name()),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_placeholder)
                                            .child(message.ago()),
                                    ),
                            )
                            .when_some(message.replies_to.as_ref(), |this, replies| {
                                this.w_full().children({
                                    let mut items = vec![];

                                    for (ix, id) in replies.iter().enumerate() {
                                        if let Some(message) = self
                                            .messages
                                            .read(cx)
                                            .iter()
                                            .find(|msg| msg.borrow().id == Some(*id))
                                            .cloned()
                                        {
                                            let message = message.borrow();

                                            items.push(
                                                div()
                                                    .id(ix)
                                                    .w_full()
                                                    .px_2()
                                                    .border_l_2()
                                                    .border_color(cx.theme().element_selected)
                                                    .text_sm()
                                                    .child(
                                                        div()
                                                            .text_color(cx.theme().text_accent)
                                                            .child(
                                                                message
                                                                    .author
                                                                    .as_ref()
                                                                    .unwrap()
                                                                    .render_name(),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .text_ellipsis()
                                                            .line_clamp(1)
                                                            .child(message.content.clone()),
                                                    )
                                                    .hover(|this| {
                                                        this.bg(cx
                                                            .theme()
                                                            .elevated_surface_background)
                                                    })
                                                    .on_click({
                                                        let id = message.id.unwrap();
                                                        cx.listener(move |this, _, _, cx| {
                                                            this.scroll_to(id, cx)
                                                        })
                                                    }),
                                            );
                                        }
                                    }

                                    items
                                })
                            })
                            .child(texts.element("body".into(), window, cx))
                            .when_some(message.errors.clone(), |this, errors| {
                                this.child(
                                    div()
                                        .id("")
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(gpui::red())
                                        .text_xs()
                                        .italic()
                                        .child(Icon::new(IconName::Info).small())
                                        .child("Failed to send message. Click to see details.")
                                        .on_click(move |_, window, cx| {
                                            let errors = errors.clone();

                                            window.open_modal(cx, move |this, _window, cx| {
                                                this.title("Error Logs")
                                                    .child(message_errors(errors.clone(), cx))
                                            });
                                        }),
                                )
                            }),
                    ),
            )
            .child(message_border(cx))
            .child(message_actions(
                vec![
                    Button::new("reply")
                        .icon(IconName::Reply)
                        .tooltip("Reply")
                        .small()
                        .ghost()
                        .on_click({
                            let message = message.clone();
                            cx.listener(move |this, _event, _window, cx| {
                                this.reply(message.clone(), cx);
                            })
                        }),
                    Button::new("copy")
                        .icon(IconName::Copy)
                        .tooltip("Copy Message")
                        .small()
                        .ghost()
                        .on_click({
                            let content = ClipboardItem::new_string(message.content.to_string());
                            cx.listener(move |_this, _event, _window, cx| {
                                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                                cx.write_to_primary(content.clone());
                                #[cfg(any(target_os = "windows", target_os = "macos"))]
                                cx.write_to_clipboard(content.clone());
                            })
                        }),
                ],
                cx,
            ))
            .on_mouse_down(gpui::MouseButton::Middle, {
                let content = ClipboardItem::new_string(message.content.to_string());
                cx.listener(move |_this, _event, _window, cx| {
                    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                    cx.write_to_primary(content.clone());
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    cx.write_to_clipboard(content.clone());
                })
            })
            .on_double_click(cx.listener({
                let message = message.clone();
                move |this, _, _window, cx| {
                    this.reply(message.clone(), cx);
                }
            }))
            .hover(|this| this.bg(cx.theme().surface_background))
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
                .child(Avatar::new(url).size(rems(1.25)))
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
            .image_cache(self.image_cache.clone())
            .size_full()
            .child(list(self.list_state.clone()).flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .relative()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .when_some(self.attaches.read(cx).as_ref(), |this, urls| {
                                this.gap_1p5()
                                    .children(urls.iter().map(|url| self.render_attach(url, cx)))
                            })
                            .when_some(self.replies_to.read(cx).as_ref(), |this, messages| {
                                this.gap_1p5().children({
                                    let mut items = vec![];

                                    for message in messages.iter() {
                                        items.push(self.render_reply(message, cx));
                                    }

                                    items
                                })
                            })
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_end()
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
                                    .child(TextInput::new(&self.input)),
                            ),
                    ),
            )
    }
}

fn message_border(cx: &App) -> Div {
    div()
        .group_hover("", |this| this.bg(cx.theme().element_active))
        .absolute()
        .left_0()
        .top_0()
        .w(px(2.))
        .h_full()
        .bg(cx.theme().border_transparent)
}

fn message_errors(errors: Vec<SendError>, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .px_3()
        .pb_3()
        .children(errors.into_iter().map(|error| {
            div()
                .text_sm()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_1()
                        .text_color(cx.theme().text_muted)
                        .child("Send to:")
                        .child(error.profile.render_name()),
                )
                .child(error.message)
        }))
}

fn message_actions(buttons: impl IntoIterator<Item = impl IntoElement>, cx: &App) -> Div {
    div()
        .group_hover("", |this| this.visible())
        .invisible()
        .absolute()
        .right_4()
        .top_neg_2()
        .shadow_sm()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .p_0p5()
        .flex()
        .gap_1()
        .children(buttons)
}
