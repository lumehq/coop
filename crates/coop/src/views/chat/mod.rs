use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::anyhow;
use common::display::DisplayProfile;
use common::nip96::nip96_upload;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, list, px, red, rems, white, Action, AnyElement, App, AppContext, ClipboardItem,
    Context, Element, Empty, Entity, EventEmitter, Flatten, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ListAlignment, ListState, MouseButton, ObjectFit,
    ParentElement, PathPromptOptions, Render, RetainAllImageCache, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, Subscription, Window,
};
use gpui_tokio::Tokio;
use i18n::t;
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::message::Message;
use registry::room::{Room, RoomKind, SendError};
use registry::Registry;
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

mod subject;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = chat, no_json)]
pub struct ChangeSubject(pub String);

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
    // System
    image_cache: Entity<RetainAllImageCache>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 2]>,
}

impl Chat {
    pub fn new(room: Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let attaches = cx.new(|_| None);
        let replies_to = cx.new(|_| None);
        let messages = cx.new(|_| vec![]);

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("chat.placeholder"))
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
                    match event {
                        InputEvent::PressEnter { .. } => {
                            this.send_message(window, cx);
                        }
                        InputEvent::Change(text) => {
                            this.mention_popup(text, input, cx);
                        }
                        _ => {}
                    };
                },
            ));

            subscriptions.push(cx.subscribe_in(
                &room,
                window,
                move |this, _, incoming, _window, cx| {
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
                },
            ));

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

    fn mention_popup(&mut self, _text: &str, _input: &Entity<InputState>, _cx: &mut Context<Self>) {
        // TODO: open mention popup at current cursor position
    }

    /// Get user input content and merged all attachments
    fn input_content(&self, cx: &Context<Self>) -> String {
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
        let Some(identity) = Identity::read_global(cx).public_key() else {
            return false;
        };

        if new_msg.author != identity {
            return false;
        }

        let min_timestamp = new_msg.created_at.as_u64().saturating_sub(10);

        self.messages
            .read(cx)
            .iter()
            .filter(|m| m.borrow().author == identity)
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
        // Return if user is not logged in
        let Some(identity) = Identity::read_global(cx).public_key() else {
            // window.push_notification("Login is required", cx);
            return;
        };

        // Get the message which includes all attachments
        let content = self.input_content(cx);
        // Get the backup setting
        let backup = AppSettings::get_backup_messages(cx);

        // Return if message is empty
        if content.trim().is_empty() {
            window.push_notification(t!("chat.empty_message_error"), cx);
            return;
        }

        // Temporary disable input
        self.input.update(cx, |this, cx| {
            this.set_loading(true, cx);
            this.set_disabled(true, cx);
        });

        // Get replies_to if it's present
        let replies = self.replies_to.read(cx).as_ref();

        // Get the current room entity
        let room = self.room.read(cx);

        // Create a temporary message for optimistic update
        let temp_message = room.create_temp_message(identity, &content, replies);

        // Create a task for sending the message in the background
        let send_message = room.send_in_background(&content, replies, backup, cx);

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
                                if let Some(msg) =
                                    this.iter().find(|msg| msg.borrow().id == id).cloned()
                                {
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
            .position(|m| m.borrow().id == id)
        {
            self.list_state.scroll_to_reveal_item(ix);
        }
    }

    fn copy_message(&self, ix: usize, cx: &Context<Self>) {
        let Some(item) = self
            .messages
            .read(cx)
            .get(ix)
            .map(|m| ClipboardItem::new_string(m.borrow().content.to_string()))
        else {
            return;
        };

        cx.write_to_clipboard(item);
    }

    fn reply_to(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(message) = self.messages.read(cx).get(ix).map(|m| m.borrow().clone()) else {
            return;
        };

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
                if let Some(ix) = replies.iter().position(|m| m.id == id) {
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

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uploading {
            return;
        }
        // Block the upload button to until current task is resolved
        self.uploading(true, cx);

        // Get the user's configured NIP96 server
        let nip96_server = AppSettings::get_media_server(cx);

        // Open native file dialog
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        let task = Tokio::spawn(cx, async move {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    if let Some(path) = paths.pop() {
                        let file = fs::read(path).await?;
                        let url = nip96_upload(nostr_client(), &nip96_server, file).await?;

                        Ok(url)
                    } else {
                        Err(anyhow!("Path not found"))
                    }
                }
                Ok(None) => Err(anyhow!("User cancelled")),
                Err(e) => Err(anyhow!("File dialog error: {e}")),
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(task.await.map_err(|e| e.into())) {
                Ok(Ok(url)) => {
                    this.update(cx, |this, cx| {
                        this.add_attachment(url, cx);
                    })
                    .ok();
                }
                Ok(Err(e)) => {
                    log::warn!("User cancelled: {e}");
                    this.update(cx, |this, cx| {
                        this.uploading(false, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            window.push_notification(e.to_string(), cx);
                            this.uploading(false, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn add_attachment(&mut self, url: Url, cx: &mut Context<Self>) {
        self.attaches.update(cx, |this, cx| {
            if let Some(model) = this.as_mut() {
                model.push(url);
            } else {
                *this = Some(vec![url]);
            }
            cx.notify();
        });
        self.uploading(false, cx);
    }

    fn remove_attachment(&mut self, url: &Url, _window: &mut Window, cx: &mut Context<Self>) {
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
                this.remove_attachment(&url, window, cx);
            }))
    }

    fn render_reply_to(&mut self, message: &Message, cx: &Context<Self>) -> impl IntoElement {
        let registry = Registry::read_global(cx);
        let profile = registry.get_person(&message.author, cx);

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
                            .child(SharedString::new(t!("chat.replying_to_label")))
                            .child(
                                div()
                                    .text_color(cx.theme().text_accent)
                                    .child(profile.display_name()),
                            ),
                    )
                    .child(
                        Button::new("remove-reply")
                            .icon(IconName::Close)
                            .xsmall()
                            .ghost()
                            .on_click({
                                let id = message.id;
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
        let Some(message) = self.messages.read(cx).get(ix).map(|m| m.borrow()) else {
            return div().id(ix);
        };

        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let hide_avatar = AppSettings::get_hide_user_avatars(cx);
        let registry = Registry::read_global(cx);
        let author = registry.get_person(&message.author, cx);

        let texts = self
            .text_data
            .entry(message.id)
            .or_insert_with(|| RichText::new(&message.content, cx));

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
                        this.child(Avatar::new(author.avatar_url(proxy)).size(rems(2.)))
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
                                            .child(author.display_name()),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_placeholder)
                                            .child(message.ago()),
                                    ),
                            )
                            .when_some(message.replies_to.as_ref(), |this, replies| {
                                this.w_full().children({
                                    let mut items = Vec::with_capacity(replies.len());
                                    let messages = self.messages.read(cx);

                                    for (ix, id) in replies.iter().cloned().enumerate() {
                                        let Some(message) = messages
                                            .iter()
                                            .map(|m| m.borrow())
                                            .find(|m| m.id == id)
                                        else {
                                            continue;
                                        };

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
                                                        .child(author.display_name()),
                                                )
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .text_ellipsis()
                                                        .line_clamp(1)
                                                        .child(message.content.clone()),
                                                )
                                                .hover(|this| {
                                                    this.bg(cx.theme().elevated_surface_background)
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.scroll_to(id, cx)
                                                })),
                                        );
                                    }

                                    items
                                })
                            })
                            .child(texts.element(ix.into(), window, cx))
                            .when_some(message.errors.as_ref(), |this, errors| {
                                this.child(self.render_message_errors(errors, cx))
                            }),
                    ),
            )
            .child(self.render_border(cx))
            .child(self.render_actions(ix, cx))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _event, _window, cx| {
                    this.copy_message(ix, cx);
                }),
            )
            .on_double_click(cx.listener({
                move |this, _event, _window, cx| {
                    this.reply_to(ix, cx);
                }
            }))
            .hover(|this| this.bg(cx.theme().surface_background))
    }

    fn render_message_errors(&self, errors: &[SendError], _cx: &Context<Self>) -> impl IntoElement {
        let errors = Rc::new(errors.to_owned());

        div()
            .id("")
            .flex()
            .items_center()
            .gap_1()
            .text_color(gpui::red())
            .text_xs()
            .italic()
            .child(Icon::new(IconName::Info).small())
            .child(SharedString::new(t!("chat.send_fail")))
            .on_click(move |_, window, cx| {
                let errors = Rc::clone(&errors);

                window.open_modal(cx, move |this, _window, cx| {
                    this.title(SharedString::new(t!("chat.logs_title"))).child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(errors.iter().map(|error| {
                                div()
                                    .text_sm()
                                    .child(
                                        div()
                                            .flex()
                                            .items_baseline()
                                            .gap_1()
                                            .text_color(cx.theme().text_muted)
                                            .child(SharedString::new(t!("chat.send_to_label")))
                                            .child(error.profile.display_name()),
                                    )
                                    .child(error.message.clone())
                            })),
                    )
                });
            })
    }

    fn render_border(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .group_hover("", |this| this.bg(cx.theme().element_active))
            .absolute()
            .left_0()
            .top_0()
            .w(px(2.))
            .h_full()
            .bg(cx.theme().border_transparent)
    }

    fn render_actions(&self, ix: usize, cx: &Context<Self>) -> impl IntoElement {
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
            .children({
                vec![
                    Button::new("reply")
                        .icon(IconName::Reply)
                        .tooltip(t!("chat.reply_button"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.reply_to(ix, cx);
                        })),
                    Button::new("copy")
                        .icon(IconName::Copy)
                        .tooltip(t!("chat.copy_message_button"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.copy_message(ix, cx);
                        })),
                ]
            })
    }
}

impl Panel for Chat {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.room.read_with(cx, |this, cx| {
            let proxy = AppSettings::get_proxy_user_avatars(cx);
            let label = this.display_name(cx);
            let url = this.display_image(proxy, cx);

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
            .tooltip(t!("chat.change_subject_button"))
            .on_click(move |_, window, cx| {
                let subject = subject::init(id, subject.clone(), window, cx);

                window.open_modal(cx, move |this, _window, _cx| {
                    this.title(SharedString::new(t!("chat.change_subject_modal_title")))
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
                                        items.push(self.render_reply_to(message, cx));
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
                                                    .icon(IconName::Upload)
                                                    .ghost()
                                                    .large()
                                                    .disabled(self.uploading)
                                                    .loading(self.uploading)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.upload(window, cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                EmojiPicker::new(self.input.downgrade())
                                                    .icon(IconName::EmojiFill)
                                                    .large(),
                                            ),
                                    )
                                    .child(TextInput::new(&self.input)),
                            ),
                    ),
            )
    }
}
